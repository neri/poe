//! i386 cpu core logic

use super::vm86::X86StackContext;
use crate::*;
use core::{
    arch::{asm, global_asm, naked_asm},
    cell::UnsafeCell,
    mem::{MaybeUninit, offset_of, size_of, transmute},
    ptr,
    sync::atomic::{Ordering, compiler_fence},
};
use paste::paste;
use x86::{gpr::Pointer32, prot::*};

pub const SYSTEM_TSS: Selector = Selector::new(1, RPL0);
pub const KERNEL_CSEL: Selector = Selector::new(2, RPL0);
pub const KERNEL_DSEL: Selector = Selector::new(3, RPL0);
#[allow(unused)]
pub const USER_CSEL: Selector = Selector::new(4, RPL3);
#[allow(unused)]
pub const USER_DSEL: Selector = Selector::new(5, RPL3);

pub struct Cpu {}

impl Cpu {
    #[inline]
    pub(crate) unsafe fn init() {
        unsafe {
            Gdt::init();
            Idt::init();
        }
    }

    /// Enter to user mode with specified stack context
    #[inline(always)]
    pub unsafe fn iret_to_user_mode(regs: &mut X86StackContext) -> ! {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            Self::_iret_to_user_mode(regs, &mut Gdt::shared().tss);
        }
    }

    #[unsafe(naked)]
    unsafe extern "fastcall" fn _iret_to_user_mode(
        regs: &mut X86StackContext,
        tss: &mut TaskStateSegment32,
    ) -> ! {
        naked_asm!(
            "mov [edx + 4], esp",

            "mov esi, ecx",
            "sub esp, {size_regs}",
            "mov edi, esp",
            "mov ecx, {size_regs} / 4",
            "rep movsd",

            ".byte 0x0f, 0xa9", // pop gs
            ".byte 0x0f, 0xa1", // pop fs
            ".byte 0x1f", // pop ds
            ".byte 0x07", // pop es
            "popad",
            "add esp, 8",
            "iretd",
            size_regs = const size_of::<X86StackContext>(),
        );
    }

    /// Fill memory with a 32-bit value using `rep stosd`.
    ///
    /// Returns the destination pointer after filling.
    ///
    /// # Safety
    ///
    /// * The DF flag must be cleared before calling this function.
    /// * Memory range safety must be guaranteed by the caller.
    #[inline(always)]
    pub unsafe fn rep_stosd(dst: *mut u32, value: u32, count: usize) -> *mut u32 {
        let mut result;
        unsafe {
            asm!(
                "rep stosd",
                inout("edi") dst => result,
                in("eax") value,
                inout("ecx") count => _,
            );
        }
        result
    }

    /// Copy memory from `src` to `dst` using `rep movsd`.
    ///
    /// Returns the destination pointer and source pointer after copying.
    ///
    /// # Safety
    ///
    /// * The DF flag must be cleared before calling this function.
    /// * Memory range safety must be guaranteed by the caller.
    #[inline(always)]
    pub unsafe fn rep_movsd(
        dst: *mut u32,
        src: *const u32,
        count: usize,
    ) -> (*mut u32, *const u32) {
        let (mut edi, mut esi) = (dst, src);
        unsafe {
            asm!(
                "xchg esi, {0}",
                "rep movsd",
                "xchg esi, {0}",
                inout(reg) esi,
                inout("edi") edi,
                inout("ecx") count => _,
            );
        }
        (edi, esi)
    }
}

macro_rules! register_exception {
    ($mnemonic:ident) => {
        paste! {
            Self::register(
                Exception::$mnemonic.as_vec(),
                [<exc_ $mnemonic>] as usize,
                DPL0,
                true,
            );
        }
    };
}

macro_rules! exception_handler {
    ($mnemonic:ident) => {
        paste! {
            unsafe extern "C" {
                fn [<exc_ $mnemonic>]() -> !;
            }

            global_asm!(
                "{label}:",
                "push {exno}",
                "jmp short _default_exception_handler",
                label = sym [<exc_ $mnemonic>],
                exno = const (Exception::$mnemonic.as_vec().0 as usize),
            );
        }
    };
}

macro_rules! exception_handler_noerr {
    ($mnemonic:ident) => {
        paste! {
            unsafe extern "C" {
                fn [<exc_ $mnemonic>]() -> !;
            }

            global_asm!(
                "{label}:",
                "push 0",
                "push {exno}",
                "jmp short _default_exception_handler",
                label = sym [<exc_ $mnemonic>],
                exno = const (Exception::$mnemonic.as_vec().0 as usize),
            );
        }
    };
}

global_asm!(
    "_default_exception_handler:",
    "cld",
    "pushad",

    // To avoid a bug in code generation that pushes segment registers
    ".byte 0x06", // push es
    ".byte 0x1e", // push ds
    ".byte 0x0f, 0xa0", // push fs
    ".byte 0x0f, 0xa8", // push gs

    "mov eax, {dsel}",
    "mov ds, eax",
    "mov es, eax",

    "mov ebp, esp",
    "and esp, 0xfffffff0",
    "mov ecx, ebp",
    "call {handler}",
    "mov esp, ebp",

    ".byte 0x0f, 0xa9", // pop gs
    ".byte 0x0f, 0xa1", // pop fs
    ".byte 0x1f", // pop ds
    ".byte 0x07", // pop es
    "popad",
    "add esp, 8",
    "iretd",
    handler = sym default_exception_handler,
    dsel = const KERNEL_DSEL.as_usize(),
);

exception_handler_noerr!(DivideError);
exception_handler_noerr!(Breakpoint);
exception_handler_noerr!(InvalidOpcode);
exception_handler_noerr!(DeviceNotAvailable);
exception_handler!(DoubleFault);
exception_handler!(GeneralProtection);
exception_handler!(PageFault);
exception_handler_noerr!(SimdException);
exception_handler_noerr!(MachineCheck);

static mut GDT: UnsafeCell<Gdt> = UnsafeCell::new(Gdt::new());
static mut IDT: UnsafeCell<Idt> = UnsafeCell::new(Idt::new());

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SetDescriptorError {
    OutOfIndex,
    PriviledgeMismatch,
}

#[repr(C, align(16))]
pub struct Gdt {
    table: [DescriptorEntry; Self::NUM_ITEMS],
    tss: TaskStateSegment32,
    iopb: [u8; 8192],
}

impl Gdt {
    pub const NUM_ITEMS: usize = 16;

    #[inline]
    const fn new() -> Self {
        unsafe { transmute(MaybeUninit::<Self>::zeroed()) }
    }

    #[inline]
    unsafe fn shared<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut GDT)).get_mut() }
    }

    unsafe fn init() {
        unsafe {
            let gdt = Self::shared();

            gdt.set_item(KERNEL_CSEL, DescriptorEntry::flat_code_segment(DPL0, USE32))
                .unwrap();
            gdt.set_item(KERNEL_DSEL, DescriptorEntry::flat_data_segment(DPL0))
                .unwrap();

            gdt.set_item(USER_CSEL, DescriptorEntry::flat_code_segment(DPL3, USE32))
                .unwrap();
            gdt.set_item(USER_DSEL, DescriptorEntry::flat_data_segment(DPL3))
                .unwrap();

            gdt.tss.ss0 = KERNEL_DSEL.as_u16() as u32;
            let iopb_base = (offset_of!(Self, iopb) - offset_of!(Self, tss)) as u16;
            gdt.tss.iopb_base = iopb_base;
            let tss_base = Linear32::new(&gdt.tss as *const _ as u32);
            let tss_limit = Limit16::new(iopb_base + 8191);
            gdt.set_item(SYSTEM_TSS, DescriptorEntry::tss32(tss_base, tss_limit))
                .unwrap();

            gdt.reload();

            // SSBL starts with a temporary GDT, so reload the selector based on our new GDT here
            asm!(
                "mov ss, {new_ss:e}",
                "push {new_cs:e}",
                // trampoline code to set new cs register
                //      call _retf
                //      jmp _next
                // _retf:
                //      retf
                // _next:
                ".byte 0xe8, 2, 0, 0, 0, 0xeb, 0x01, 0xcb",

                "mov ds, {new_ss:e}",
                "mov es, {new_ss:e}",
                "mov fs, {new_ss:e}",
                "mov gs, {new_ss:e}",
                new_ss = in(reg) KERNEL_DSEL.as_usize(),
                new_cs = in(reg) KERNEL_CSEL.as_usize(),
            );

            asm!("ltr {0:x}", in(reg) SYSTEM_TSS.0,);
        }
    }

    #[inline]
    pub unsafe fn set_item(
        &mut self,
        selector: Selector,
        desc: DescriptorEntry,
    ) -> Result<(), SetDescriptorError> {
        let index = selector.index();
        if selector.rpl() != desc.dpl().as_rpl() {
            return Err(SetDescriptorError::PriviledgeMismatch);
        }
        self.table
            .get_mut(index)
            .map(|v| *v = desc)
            .ok_or(SetDescriptorError::OutOfIndex)
    }

    /// Reload GDT
    unsafe fn reload(&self) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            asm!(
                "push {0}",
                "push {1}",
                "lgdt [esp + 2]",
                "add esp, 8",
                in(reg) &self.table,
                in(reg) ((self.table.len() * 8 - 1) << 16),
            );
        }
        compiler_fence(Ordering::SeqCst);
    }

    #[inline]
    pub unsafe fn set_tss_esp0(esp: u32) {
        unsafe {
            let gdt = Self::shared();
            ptr::addr_of_mut!(gdt.tss.esp0).write_volatile(esp);
        }
    }

    #[inline]
    pub fn get_tss_esp0() -> u32 {
        unsafe {
            let gdt = Self::shared();
            ptr::addr_of!(gdt.tss.esp0).read_volatile()
        }
    }
}

#[repr(C, align(16))]
pub struct Idt {
    table: [DescriptorEntry; Self::MAX],
    exceptions: [Option<fn(&mut X86StackContext) -> bool>; 32],
}

impl Idt {
    pub const MAX: usize = 256;

    const fn new() -> Self {
        Self {
            table: [DescriptorEntry::null(); Self::MAX],
            exceptions: [None; 32],
        }
    }

    unsafe fn init() {
        unsafe {
            let idt = Self::shared();

            register_exception!(DivideError);
            register_exception!(Breakpoint);
            register_exception!(InvalidOpcode);
            register_exception!(DeviceNotAvailable);
            register_exception!(DoubleFault);
            register_exception!(GeneralProtection);
            register_exception!(PageFault);
            register_exception!(MachineCheck);
            register_exception!(SimdException);

            idt.load();
        }
    }

    #[inline]
    unsafe fn shared<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut IDT)).get_mut() }
    }

    #[inline]
    unsafe fn load(&self) {
        unsafe {
            asm!(
                "push {0}",
                "push {1}",
                "lidt [esp+2]",
                "add esp, 8",
                in(reg) &self.table,
                in(reg) ((self.table.len() * 8 - 1) << 16),
            );
        }
    }

    pub unsafe fn register(vec: InterruptVector, offset: usize, dpl: DPL, is_inter: bool) {
        let entry = DescriptorEntry::gate32(
            offset,
            KERNEL_CSEL,
            dpl,
            if is_inter {
                DescriptorType::InterruptGate
            } else {
                DescriptorType::TrapGate
            },
        );
        unsafe {
            Self::shared().table[vec.0 as usize] = entry;
        }
    }

    #[inline]
    pub unsafe fn handle_exception(exc: Exception, handler: fn(&mut X86StackContext) -> bool) {
        unsafe {
            Self::shared().exceptions[exc.as_vec().0 as usize] = Some(handler);
        }
    }
}

unsafe extern "fastcall" fn default_exception_handler(ctx: &mut X86StackContext) {
    if unsafe { Idt::shared() }.exceptions[ctx.vector().0 as usize].map_or(false, |f| f(ctx)) {
        return;
    }

    let stderr = System::stderr();
    stderr.set_attribute(0x1f);

    let is_vm = ctx.is_vm();

    let ss = ctx.ss3().unwrap_or(Selector::NULL);
    let esp = ctx
        .esp3()
        .unwrap_or_else(|| Pointer32::from_u32(&ctx.esp3() as *const _ as u32));
    let ds = ctx.vmds().unwrap_or(ctx.ds());
    let es = ctx.vmes().unwrap_or(ctx.es());

    let _ = writeln!(
        stderr,
        "#### EXCEPTION {:02x}-{:04x}",
        ctx.vector().0,
        ctx.error_code(),
    );
    if is_vm {
        let _ = writeln!(
            stderr,
            "CS:IP {:04x}:{:04x} SS:SP {:04x}:{:04x}",
            ctx.cs().0,
            ctx.eip.as_u16(),
            ss,
            esp.as_u16(),
        );
    } else {
        let _ = writeln!(
            stderr,
            "CS:EIP {:02x}:{:08x} SS:ESP {:02x}:{:08x}",
            ctx.cs(),
            ctx.eip.as_u32(),
            ss,
            esp.as_u32(),
        );
    }
    let _ = writeln!(
        stderr,
        "EAX {:08x} EBX {:08x} ECX {:08x} EDX {:08x} ESI {:08x} EDI {:08x}",
        ctx.eax.d(),
        ctx.ebx.d(),
        ctx.ecx.d(),
        ctx.edx.d(),
        ctx.esi.d(),
        ctx.edi.d(),
    );
    let _ = writeln!(
        stderr,
        "EBP {:08x} DS {:04x} ES {:04x} EFL {}",
        ctx.ebp.d(),
        ds,
        es,
        ctx.eflags,
    );

    Hal::cpu().stop();
}

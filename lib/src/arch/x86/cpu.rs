//! i386 cpu core logic

use crate::*;
use core::{
    arch::{asm, global_asm, naked_asm},
    cell::UnsafeCell,
    mem::{MaybeUninit, offset_of, transmute},
    ptr,
    sync::atomic::{Ordering, compiler_fence},
};
use paste::paste;
use x86::{gpr::Eflags, prot::*};

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
            size_regs = const core::mem::size_of::<X86StackContext>(),
        );
    }
}

macro_rules! register_exception {
    ($mnemonic:ident) => {
        paste! {
            Self::register(
                ExceptionType::$mnemonic.as_vec(),
                [<exc_ $mnemonic>] as usize,
                DPL0,
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
                exno = const (ExceptionType::$mnemonic.as_vec().0 as usize),
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
                exno = const (ExceptionType::$mnemonic.as_vec().0 as usize),
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
    "mov ds, ax",
    "mov es, ax",

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
            let tss_base = Linear32(&gdt.tss as *const _ as u32);
            let tss_limit = Limit16(iopb_base + 8191);
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

    pub unsafe fn register(vec: InterruptVector, offset: usize, dpl: DPL) {
        let entry = DescriptorEntry::gate32(
            offset,
            KERNEL_CSEL,
            dpl,
            if dpl == DPL0 {
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
    pub unsafe fn handle_exception(exc: ExceptionType, handler: fn(&mut X86StackContext) -> bool) {
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
        .unwrap_or_else(|| &ctx.esp3() as *const _ as usize as u32);
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
            ctx.eip,
            ss,
            esp,
        );
    } else {
        let _ = writeln!(
            stderr,
            "CS:EIP {:02x}:{:08x} SS:ESP {:02x}:{:08x}",
            ctx.cs(),
            ctx.eip,
            ss,
            esp,
        );
    }
    let _ = writeln!(
        stderr,
        "EAX {:08x} EBX {:08x} ECX {:08x} EDX {:08x} ESI {:08x} EDI {:08x}",
        ctx.eax, ctx.ebx, ctx.ecx, ctx.edx, ctx.esi, ctx.edi,
    );
    let _ = writeln!(
        stderr,
        "EBP {:08x} DS {:04x} ES {:04x} EFL {}",
        ctx.ebp, ds, es, ctx.eflags,
    );

    Hal::cpu().stop();
}

#[allow(dead_code)]
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct X86StackContext {
    _es: u32,
    _ds: u32,
    _fs: u32,
    _gs: u32,

    pub edi: u32,
    pub esi: u32,
    pub ebp: u32,
    _esp_dummy: u32,
    pub ebx: u32,
    pub edx: u32,
    pub ecx: u32,
    pub eax: u32,

    // `vector` and `error_code` are set only when an exception occurs
    _vector: u32,
    _error_code: u32,

    pub eip: u32,
    _cs: u32,
    pub eflags: Eflags,

    // Valid only if `is_user()` is `true` after this point.
    _esp3: u32,
    _ss3: u32,

    // Valid only if `is_vm()` is `true` after this point.
    _vmes: u32,
    _vmds: u32,
    _vmfs: u32,
    _vmgs: u32,
}

#[allow(unused)]
impl X86StackContext {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            _es: 0,
            _ds: 0,
            _fs: 0,
            _gs: 0,
            edi: 0,
            esi: 0,
            ebp: 0,
            _esp_dummy: 0,
            ebx: 0,
            edx: 0,
            ecx: 0,
            eax: 0,
            _vector: 0,
            _error_code: 0,
            eip: 0,
            _cs: 0,
            eflags: Eflags::empty(),
            _esp3: 0,
            _ss3: 0,
            _vmes: 0,
            _vmds: 0,
            _vmfs: 0,
            _vmgs: 0,
        }
    }

    #[inline]
    pub fn is_vm(&self) -> bool {
        self.eflags.contains(Eflags::VM)
    }

    #[inline]
    pub fn is_user(&self) -> bool {
        self.is_vm() || (self.cs().rpl() != RPL0)
    }

    #[inline]
    pub const fn cs(&self) -> Selector {
        Selector(self._cs as u16)
    }

    #[inline]
    pub const fn ds(&self) -> Selector {
        Selector(self._ds as u16)
    }

    #[inline]
    pub const fn es(&self) -> Selector {
        Selector(self._es as u16)
    }

    #[inline]
    pub const fn fs(&self) -> Selector {
        Selector(self._fs as u16)
    }

    #[inline]
    pub const fn gs(&self) -> Selector {
        Selector(self._gs as u16)
    }

    #[inline]
    pub const fn error_code(&self) -> u16 {
        self._error_code as u16
    }

    #[inline]
    pub const fn selector_error_code(&self) -> SelectorErrorCode {
        SelectorErrorCode(self._error_code as u16)
    }

    #[inline]
    pub const fn vector(&self) -> InterruptVector {
        InterruptVector(self._vector as u8)
    }

    #[inline]
    pub fn vmds(&self) -> Option<Selector> {
        if self.is_vm() {
            Some(Selector(self._vmds as u16))
        } else {
            None
        }
    }

    #[inline]
    pub fn vmes(&self) -> Option<Selector> {
        if self.is_vm() {
            Some(Selector(self._vmes as u16))
        } else {
            None
        }
    }

    #[inline]
    pub fn vmfs(&self) -> Option<Selector> {
        if self.is_vm() {
            Some(Selector(self._vmfs as u16))
        } else {
            None
        }
    }

    #[inline]
    pub fn vmgs(&self) -> Option<Selector> {
        if self.is_vm() {
            Some(Selector(self._vmgs as u16))
        } else {
            None
        }
    }

    #[inline]
    pub unsafe fn vmds_unchecked(&self) -> Selector {
        Selector(self._vmds as u16)
    }

    #[inline]
    pub unsafe fn vmes_unchecked(&self) -> Selector {
        Selector(self._vmes as u16)
    }

    #[inline]
    pub unsafe fn vmfs_unchecked(&self) -> Selector {
        Selector(self._vmfs as u16)
    }

    #[inline]
    pub unsafe fn vmgs_unchecked(&self) -> Selector {
        Selector(self._vmgs as u16)
    }

    #[inline]
    pub unsafe fn set_vmds(&mut self, vmds: Selector) {
        self._vmds = vmds.as_u16() as u32;
    }

    #[inline]
    pub unsafe fn set_vmes(&mut self, vmes: Selector) {
        self._vmes = vmes.as_u16() as u32;
    }

    #[inline]
    pub unsafe fn set_vmfs(&mut self, vmfs: Selector) {
        self._vmfs = vmfs.as_u16() as u32;
    }

    #[inline]
    pub unsafe fn set_vmgs(&mut self, vmgs: Selector) {
        self._vmgs = vmgs.as_u16() as u32;
    }

    #[inline]
    pub fn set_cs(&mut self, cs: Selector) {
        self._cs = cs.as_u16() as u32;
    }

    #[inline]
    pub unsafe fn set_esp3(&mut self, esp3: u32) {
        self._esp3 = esp3;
    }

    #[inline]
    pub unsafe fn esp3_unchecked(&self) -> u32 {
        self._esp3
    }

    #[inline]
    pub unsafe fn fix_esp3<F>(&mut self, f: F)
    where
        F: FnOnce(u32) -> u32,
    {
        self._esp3 = f(self._esp3);
    }

    #[inline]
    pub fn esp3(&self) -> Option<u32> {
        if self.is_user() {
            Some(self._esp3)
        } else {
            None
        }
    }

    #[inline]
    pub unsafe fn ss3_unchecked(&self) -> Selector {
        Selector(self._ss3 as u16)
    }

    #[inline]
    pub fn ss3(&self) -> Option<Selector> {
        if self.is_user() {
            Some(Selector(self._ss3 as u16))
        } else {
            None
        }
    }

    #[inline]
    pub unsafe fn set_ss3(&mut self, ss3: Selector) {
        self._ss3 = ss3.as_u16() as u32;
    }

    #[inline]
    pub fn ss_esp3(&self) -> Option<(Selector, u32)> {
        if self.is_user() {
            Some((Selector(self._ss3 as u16), self._esp3))
        } else {
            None
        }
    }

    #[inline]
    pub unsafe fn ss_esp3_unchecked(&self) -> (Selector, u32) {
        (Selector(self._ss3 as u16), self._esp3)
    }

    #[inline]
    pub fn al(&self) -> u8 {
        self.eax as u8
    }

    #[inline]
    pub fn ah(&self) -> u8 {
        (self.eax >> 8) as u8
    }

    #[inline]
    pub fn bl(&self) -> u8 {
        self.ebx as u8
    }

    #[inline]
    pub fn bh(&self) -> u8 {
        (self.ebx >> 8) as u8
    }

    #[inline]
    pub fn cl(&self) -> u8 {
        self.ecx as u8
    }

    #[inline]
    pub fn ch(&self) -> u8 {
        (self.ecx >> 8) as u8
    }

    #[inline]
    pub fn dl(&self) -> u8 {
        self.edx as u8
    }

    #[inline]
    pub fn dh(&self) -> u8 {
        (self.edx >> 8) as u8
    }

    #[inline]
    pub fn set_al(&mut self, al: u8) {
        self.eax = (self.eax & 0xffffff00) | al as u32;
    }

    #[inline]
    pub fn set_ah(&mut self, ah: u8) {
        self.eax = (self.eax & 0xffff00ff) | (ah as u32) << 8;
    }

    #[inline]
    pub fn set_bl(&mut self, bl: u8) {
        self.ebx = (self.ebx & 0xffffff00) | bl as u32;
    }

    #[inline]
    pub fn set_bh(&mut self, bh: u8) {
        self.ebx = (self.ebx & 0xffff00ff) | (bh as u32) << 8;
    }

    #[inline]
    pub fn set_cl(&mut self, cl: u8) {
        self.ecx = (self.ecx & 0xffffff00) | cl as u32;
    }

    #[inline]
    pub fn set_ch(&mut self, ch: u8) {
        self.ecx = (self.ecx & 0xffff00ff) | (ch as u32) << 8;
    }

    #[inline]
    pub fn set_dl(&mut self, dl: u8) {
        self.edx = (self.edx & 0xffffff00) | dl as u32;
    }

    #[inline]
    pub fn set_dh(&mut self, dh: u8) {
        self.edx = (self.edx & 0xffff00ff) | (dh as u32) << 8;
    }
}

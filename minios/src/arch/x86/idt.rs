//! Interrupt Descriptor Table

use super::vm86::X86StackContext;
use crate::arch::gdt::{KERNEL_CSEL, KERNEL_DSEL};
use crate::*;
use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use paste::paste;
use x86::{gpr::Pointer32, prot::*};

static mut IDT: UnsafeCell<Idt> = UnsafeCell::new(Idt::new());

#[repr(C, align(16))]
pub struct Idt {
    table: [DescriptorEntry; Self::MAX],
    exception_chains: [Option<unsafe fn(&mut X86StackContext) -> bool>; 32],
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

impl Idt {
    pub const MAX: usize = 256;

    const fn new() -> Self {
        Self {
            table: [DescriptorEntry::NULL; Self::MAX],
            exception_chains: [None; 32],
        }
    }

    pub(super) unsafe fn init() {
        unsafe {
            let idt = Self::shared();

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
    pub unsafe fn handle_exception(
        exc: Exception,
        handler: unsafe fn(&mut X86StackContext) -> bool,
    ) {
        unsafe {
            Self::shared().exception_chains[exc.as_vec().0 as usize] = Some(handler);
        }
    }
}

unsafe extern "fastcall" fn default_exception_handler(ctx: &mut X86StackContext) {
    let idt = unsafe { Idt::shared() };
    if idt.exception_chains[ctx.vector().0 as usize].map_or(false, |f| unsafe { f(ctx) }) {
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
            ctx.cs(),
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

    Hal::cpu().halt();
}

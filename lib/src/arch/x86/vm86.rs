//! Simple Virtual 8086 Mode Manager

use super::{
    cpu::{Cpu, Gdt, Idt, X86StackContext},
    lomem::{LowMemoryManager, ManagedLowMemory},
    setjmp::JmpBuf,
};
use crate::*;
use core::{cell::UnsafeCell, num::NonZeroUsize, ptr::null_mut};
use x86::{
    gpr::Eflags,
    prot::{ExceptionType, IOPL, InterruptVector, Linear32, Selector},
    real::{Far16Ptr, Offset16},
};

static mut VMM: UnsafeCell<VM86> = UnsafeCell::new(VM86::new());

/// Simple Virtual 8086 Mode Manager
pub struct VM86 {
    vmbp: Linear32,
    vmbp_csip: Far16Ptr,
    vm_stack: Option<ManagedLowMemory>,
    jmp_buf: JmpBuf,
    context: *mut X86StackContext,
}

impl VM86 {
    /// IOPL in virtual 8086 mode
    const IOPL_VM: IOPL = IOPL::SUPERVISOR;

    #[inline]
    const fn new() -> Self {
        Self {
            vmbp: Linear32::NULL,
            vmbp_csip: Far16Ptr::NULL,
            vm_stack: None,
            jmp_buf: JmpBuf::new(),
            context: null_mut(),
        }
    }

    pub(crate) unsafe fn init() {
        unsafe {
            let shared = Self::shared_mut();
            shared.vm_stack = LowMemoryManager::alloc_page_checked();

            let mut vmbp = Linear32::NULL;

            // Find ARPL as VMBP
            for p in 0x0f_0000..0x0f_fff0 {
                let p = p as *mut u8;
                if p.read_volatile() == 0x63 {
                    vmbp = Linear32(p as u32);
                    break;
                }
            }

            if vmbp == Linear32::NULL {
                // TODO: other methods
                panic!("VMBP not found");
            }
            shared.vmbp = vmbp;
            shared.vmbp_csip = Far16Ptr::from_linear(shared.vmbp);

            Idt::handle_exception(ExceptionType::InvalidOpcode, Self::_handle_ud);
            Idt::handle_exception(ExceptionType::GeneralProtection, Self::_handle_gpf);
        }
    }

    #[inline]
    unsafe fn shared_mut<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut VMM)).get_mut() }
    }

    /// Invokes virtual 8086 mode with INT instruction executed.
    /// Typically used for BIOS calls.
    ///
    #[allow(unused)]
    pub unsafe fn call_bios(int_vec: InterruptVector, ctx: &mut X86StackContext) {
        unsafe {
            let guard = Hal::cpu().interrupt_guard();
            let shared = Self::shared_mut();
            let old_vm_stack = shared.vm_stack.take();
            let old_jmp_buf = shared.jmp_buf.clone();
            let old_context = shared.context;
            let old_tss_esp0 = Gdt::get_tss_esp0();

            let mut temp_stack = None;
            let vm_stack = match old_vm_stack.as_ref() {
                Some(v) => v,
                None => {
                    temp_stack = LowMemoryManager::alloc_page().into();
                    temp_stack.as_ref().unwrap()
                }
            };

            Self::set_vm_eflags(ctx, ctx.eflags);
            ctx.set_ss3(vm_stack.sel());
            ctx.set_esp3((vm_stack.limit().0 & 0xfffe) as u32);
            ctx.set_cs(shared.vmbp_csip.sel());
            ctx.eip = shared.vmbp_csip.off().0 as u32;
            Self::redirect_vm_interrupt(int_vec, ctx, true);

            shared.context = ctx;
            if shared.jmp_buf.set_jmp().is_none() {
                Cpu::iret_to_user_mode(ctx);
            }

            Gdt::set_tss_esp0(old_tss_esp0);
            shared.jmp_buf = old_jmp_buf;
            shared.context = old_context;
            shared.vm_stack = old_vm_stack;
            drop(guard);
        }
    }

    /// Invokes virtual 8086 mode with FAR CALL instruction executed.
    ///
    #[allow(unused)]
    pub unsafe fn call_far(target: Far16Ptr, ctx: &mut X86StackContext) {
        unsafe {
            let guard = Hal::cpu().interrupt_guard();
            let shared = Self::shared_mut();
            let old_vm_stack = shared.vm_stack.take();
            let old_jmp_buf = shared.jmp_buf.clone();
            let old_context = shared.context;
            let old_tss_esp0 = Gdt::get_tss_esp0();

            let mut temp_stack = None;
            let vm_stack = match old_vm_stack.as_ref() {
                Some(v) => v,
                None => {
                    temp_stack = LowMemoryManager::alloc_page().into();
                    temp_stack.as_ref().unwrap()
                }
            };

            Self::set_vm_eflags(ctx, ctx.eflags);
            ctx.set_ss3(vm_stack.sel());
            ctx.set_esp3((vm_stack.limit().0 & 0xfffe) as u32);
            Self::vm_push16(ctx, shared.vmbp_csip.sel().as_u16());
            Self::vm_push16(ctx, shared.vmbp_csip.off().0);
            ctx.set_cs(target.sel());
            ctx.eip = target.off().0 as u32;

            shared.context = ctx;
            if shared.jmp_buf.set_jmp().is_none() {
                Cpu::iret_to_user_mode(ctx);
            }

            Gdt::set_tss_esp0(old_tss_esp0);
            shared.jmp_buf = old_jmp_buf;
            shared.context = old_context;
            shared.vm_stack = old_vm_stack;
            drop(guard);
        }
    }

    unsafe fn intercept_vmbp(&mut self, ctx: &X86StackContext) -> ! {
        unsafe {
            self.context.write_volatile(ctx.clone());
            self.jmp_buf.long_jmp(NonZeroUsize::new(1).unwrap());
        }
    }

    /// Handles #UD exception in virtual 8086 mode.
    fn _handle_ud(ctx: &mut X86StackContext) -> bool {
        unsafe {
            if ctx.is_vm() {
                let vm_csip = Far16Ptr::new(ctx.cs(), Offset16(ctx.eip as u16)).as_linear();
                let shared = Self::shared_mut();
                if shared.vmbp == vm_csip {
                    shared.intercept_vmbp(ctx);
                }
            }
        }
        return false;
    }

    /// Handles #GP exception in virtual 8086 mode.
    fn _handle_gpf(ctx: &mut X86StackContext) -> bool {
        unsafe {
            if ctx.is_vm() {
                let vm_csip = Far16Ptr::new(ctx.cs(), Offset16(ctx.eip as u16)).as_linear();
                let shared = Self::shared_mut();
                if shared.vmbp == vm_csip {
                    shared.intercept_vmbp(ctx);
                }

                let err = ctx.selector_error_code();
                if let Some(int_vec) = err.int_vec() {
                    Self::redirect_vm_interrupt(int_vec, ctx, false);
                    return true;
                } else {
                    return Self::simulate_vm_instruction(ctx);
                }
            }
        }
        return false;
    }

    /// Redirects interrupt to virtual 8086 mode.
    ///
    /// Redirect interrupts on the stack if the context is already running in virtual 8086 mode,
    /// otherwise (running in protected mode) invoke virtual 8086 mode.
    pub unsafe fn redirect_interrupt(int_vec: InterruptVector, ctx: &mut X86StackContext) {
        unsafe {
            if ctx.is_vm() {
                Self::redirect_vm_interrupt(int_vec, ctx, true);
            } else {
                let mut regs = X86StackContext::default();
                Self::call_bios(int_vec, &mut regs);
            }
        }
    }

    /// Redirect interrupts by adjusting the stack context in virtual 8086 mode.
    unsafe fn redirect_vm_interrupt(
        int_vec: InterruptVector,
        ctx: &mut X86StackContext,
        is_external: bool,
    ) {
        unsafe {
            let vm_csip = Self::vm_csip_ptr(ctx);
            let mut skip = 0;
            if is_external || ctx.selector_error_code().is_external() {
                // external int
            } else {
                match vm_csip.read_volatile() {
                    0xCD => {
                        if vm_csip.add(1).read_volatile() == int_vec.0 {
                            // CD: int N
                            skip = 2;
                        }
                    }
                    0xCC => {
                        if int_vec == ExceptionType::Breakpoint.as_vec() {
                            // CC: int3
                            skip = 1;
                        }
                    }
                    _ => {}
                }
            }

            Self::vm_push16(ctx, ctx.eflags.bits() as u16);
            Self::vm_push16(ctx, ctx.cs().as_u16());
            Self::vm_push16(ctx, (ctx.eip as u16).wrapping_add(skip));

            let vm_intvect = (((int_vec.0 as usize) << 2) as *const u32).read_volatile();
            ctx.eip = vm_intvect & 0xFFFF;
            ctx.set_cs(Selector((vm_intvect >> 16) as u16));
        }
    }

    /// In virtual 86 mode, some instructions need to be simulated.
    ///
    /// - parameter ctx: Stack context
    /// - returns: `true` if the instruction was successfully processed
    ///
    /// More accurate emulation is needed to create an OS, but POE is not an OS, so we cut corners.
    #[must_use]
    unsafe fn simulate_vm_instruction(ctx: &mut X86StackContext) -> bool {
        unsafe {
            let vm_csip = Self::vm_csip_ptr(ctx);
            let mut skip = 0;
            let mut prefix_66 = false;

            loop {
                let prefix = vm_csip.add(skip).read_volatile();
                if prefix == 0x66 {
                    prefix_66 = true;
                    skip += 1;
                } else {
                    break;
                }
            }

            match vm_csip.add(skip).read_volatile() {
                0x9C => {
                    // PUSHF
                    if prefix_66 {
                        Self::vm_push32(ctx, ctx.eflags.bits() as u32);
                    } else {
                        Self::vm_push16(ctx, ctx.eflags.bits() as u16);
                    }
                    skip += 1;
                }
                0x9D => {
                    // POPF
                    let new_fl: Eflags;
                    if prefix_66 {
                        new_fl = Eflags::from_bits(Self::vm_pop32(ctx) as usize);
                    } else {
                        new_fl = Eflags::from_bits(
                            (ctx.eflags.bits() & 0xffff_0000) | Self::vm_pop16(ctx) as usize,
                        );
                    }
                    Self::set_vm_eflags(ctx, new_fl);
                    skip += 1;
                }
                0xCD => {
                    // INT N
                    let int_vec = InterruptVector(vm_csip.add(skip + 1).read_volatile() as u8);
                    Self::redirect_vm_interrupt(int_vec, ctx, false);
                    return true;
                }
                0xCC => {
                    // INT3
                    Self::redirect_vm_interrupt(ExceptionType::Breakpoint.as_vec(), ctx, false);
                    return true;
                }
                0xCF => {
                    // IRET
                    let new_cs: Selector;
                    let new_fl: Eflags;
                    if prefix_66 {
                        ctx.eip = Self::vm_pop32(ctx);
                        new_cs = Selector(Self::vm_pop32(ctx) as u16);
                        new_fl = Eflags::from_bits(Self::vm_pop32(ctx) as usize);
                    } else {
                        ctx.eip = Self::vm_pop16(ctx) as u32;
                        new_cs = Selector(Self::vm_pop16(ctx));
                        new_fl = Eflags::from_bits(
                            (ctx.eflags.bits() & 0xffff_0000) | Self::vm_pop16(ctx) as usize,
                        );
                    }
                    ctx.set_cs(new_cs);
                    Self::set_vm_eflags(ctx, new_fl);
                    return true;
                }
                0xF4 => {
                    // HLT
                    Hal::cpu().enable_interrupt();
                    Hal::cpu().wait_for_interrupt();
                    skip += 1;
                }
                0xFA => {
                    // CLI
                    ctx.eflags.remove(Eflags::IF);
                    skip += 1;
                }
                0xFB => {
                    // STI
                    ctx.eflags.insert(Eflags::IF);
                    skip += 1;
                }
                _ => {
                    // Unsupported
                    return false;
                }
            }

            ctx.eip = ctx.eip.wrapping_add(skip as u32) & 0xFFFF;
            return true;
        }
    }

    #[inline]
    pub fn vm_csip_ptr(ctx: &X86StackContext) -> *mut u8 {
        Self::far16_to_ptr(ctx.cs(), Offset16(ctx.eip as u16))
    }

    #[inline]
    pub unsafe fn vm_sssp_ptr(ctx: &X86StackContext) -> *mut u16 {
        let (ss, esp) = unsafe { ctx.ss_esp3_unchecked() };
        Self::far16_to_ptr(ss, Offset16(esp as u16))
    }

    #[inline]
    pub unsafe fn vm_sssp_ptr32(ctx: &X86StackContext) -> *mut u32 {
        let (ss, esp) = unsafe { ctx.ss_esp3_unchecked() };
        Self::far16_to_ptr(ss, Offset16(esp as u16))
    }

    #[inline]
    pub fn far16_to_ptr<T>(sel: Selector, off: Offset16) -> *mut T {
        Far16Ptr::new(sel, off).as_linear().0 as *mut T
    }

    #[inline]
    pub unsafe fn vm_push16(ctx: &mut X86StackContext, value: u16) {
        unsafe {
            ctx.fix_esp3(|esp3| esp3.wrapping_sub(size_of_val(&value) as u32) & 0xffff);
            Self::vm_sssp_ptr(ctx).write_volatile(value);
        }
    }

    #[inline]
    pub unsafe fn vm_pop16(ctx: &mut X86StackContext) -> u16 {
        unsafe {
            let value = Self::vm_sssp_ptr(ctx).read_volatile();
            ctx.fix_esp3(|esp3| esp3.wrapping_add(size_of_val(&value) as u32) & 0xffff);
            value
        }
    }

    #[inline]
    pub unsafe fn vm_push32(ctx: &mut X86StackContext, value: u32) {
        unsafe {
            ctx.fix_esp3(|esp3| esp3.wrapping_sub(size_of_val(&value) as u32) & 0xffff);
            Self::vm_sssp_ptr32(ctx).write_volatile(value);
        }
    }

    #[inline]
    pub unsafe fn vm_pop32(ctx: &mut X86StackContext) -> u32 {
        unsafe {
            let value = Self::vm_sssp_ptr32(ctx).read_volatile();
            ctx.fix_esp3(|esp3| esp3.wrapping_add(size_of_val(&value) as u32) & 0xffff);
            value
        }
    }

    #[inline]
    pub fn set_vm_eflags(ctx: &mut X86StackContext, eflags: Eflags) {
        let mut eflags = eflags.canonicalized();
        eflags.insert(Eflags::VM);
        eflags.insert(Eflags::IF);
        eflags.set_iopl(Self::IOPL_VM);
        ctx.eflags = eflags;
    }
}

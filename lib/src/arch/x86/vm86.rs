//! Simple Virtual 8086 Mode Manager

use super::{
    cpu::{Cpu, Gdt, Idt},
    lomem::{LoMemoryManager, ManagedLowMemory},
    setjmp::JmpBuf,
};
use crate::*;
use core::{cell::UnsafeCell, num::NonZeroUsize, ptr::null_mut};
use x86::prot::{RPL0, SelectorErrorCode};
use x86::{
    gpr::Eflags,
    prot::{Exception, IOPL, InterruptVector, Linear32, Selector},
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
            shared.vm_stack = LoMemoryManager::alloc_page_checked();

            let mut vmbp = Linear32::NULL;

            // Find ARPL as VMBP
            for p in 0x0f_0000..0x0f_fff0 {
                let p = p as *mut u8;
                if p.read_volatile() == 0x63 {
                    vmbp = Linear32::new(p as u32);
                    break;
                }
            }

            if vmbp == Linear32::NULL {
                // TODO: other methods
                panic!("VMBP not found");
            }
            shared.vmbp = vmbp;
            shared.vmbp_csip = Far16Ptr::from_linear(shared.vmbp);

            Idt::handle_exception(Exception::InvalidOpcode, Self::_handle_ud);
            Idt::handle_exception(Exception::GeneralProtection, Self::_handle_gpf);
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
                    temp_stack = LoMemoryManager::alloc_page().into();
                    temp_stack.as_ref().unwrap()
                }
            };

            ctx.adjust_vm_eflags();
            ctx.set_ss3(vm_stack.sel());
            ctx.set_esp3(vm_stack.limit().as_u32() & 0xfffe);
            ctx.set_cs(shared.vmbp_csip.sel());
            ctx.eip = shared.vmbp_csip.off().as_u32();
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
                    temp_stack = LoMemoryManager::alloc_page().into();
                    temp_stack.as_ref().unwrap()
                }
            };

            ctx.adjust_vm_eflags();
            ctx.set_ss3(vm_stack.sel());
            ctx.set_esp3(vm_stack.limit().as_u32() & 0xfffe);
            ctx.vm_push16(shared.vmbp_csip.sel().as_u16());
            ctx.vm_push16(shared.vmbp_csip.off().as_u16());
            ctx.set_cs(target.sel());
            ctx.eip = target.off().as_u32();

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
                let vm_csip = Far16Ptr::new(ctx.cs(), Offset16::new(ctx.eip as u16)).to_linear();
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
                let vm_csip = Far16Ptr::new(ctx.cs(), Offset16::new(ctx.eip as u16)).to_linear();
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
            let mut skip = 0;
            if is_external || ctx.selector_error_code().is_external() {
                // external int
            } else {
                let vm_csip = ctx.vm_csip_ptr();
                match vm_csip.read_volatile() {
                    0xCD => {
                        if vm_csip.add(1).read_volatile() == int_vec.0 {
                            // CD: int N
                            skip = 2;
                        }
                    }
                    0xCC => {
                        if int_vec == Exception::Breakpoint.as_vec() {
                            // CC: int3
                            skip = 1;
                        }
                    }
                    _ => {}
                }
            }

            ctx.vm_push16(ctx.eflags.bits() as u16);
            ctx.vm_push16(ctx.cs().as_u16());
            ctx.vm_push16((ctx.eip as u16).wrapping_add(skip));

            let vm_intvec =
                Far16Ptr::from_u32((((int_vec.0 as usize) << 2) as *const u32).read_volatile());
            ctx.eip = vm_intvec.off().as_u32();
            ctx.set_cs(vm_intvec.sel());
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
            let vm_csip = ctx.vm_csip_ptr();
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
                    // 9C: PUSHF
                    let eflags = ctx.vm_eflags();
                    if prefix_66 {
                        ctx.vm_push32(eflags.bits() as u32);
                    } else {
                        ctx.vm_push16(eflags.bits() as u16);
                    }
                    skip += 1;
                }
                0x9D => {
                    // 9D: POPF
                    let new_fl: Eflags;
                    if prefix_66 {
                        new_fl = Eflags::from_bits(ctx.vm_pop32() as usize);
                    } else {
                        new_fl = Eflags::from_bits(
                            (ctx.eflags.bits() & 0xffff_0000) | ctx.vm_pop16() as usize,
                        );
                    }
                    ctx.set_vm_eflags(new_fl);
                    skip += 1;
                }
                0xCD => {
                    // CD nn: INT N
                    let int_vec = InterruptVector(vm_csip.add(skip + 1).read_volatile() as u8);
                    Self::redirect_vm_interrupt(int_vec, ctx, false);
                    return true;
                }
                0xCC => {
                    // CC: INT3
                    Self::redirect_vm_interrupt(Exception::Breakpoint.as_vec(), ctx, false);
                    return true;
                }
                0xCF => {
                    // CF: IRET
                    let new_cs: Selector;
                    let new_fl: Eflags;
                    if prefix_66 {
                        ctx.eip = ctx.vm_pop32();
                        new_cs = Selector(ctx.vm_pop32() as u16);
                        new_fl = Eflags::from_bits(ctx.vm_pop32() as usize);
                    } else {
                        ctx.eip = ctx.vm_pop16() as u32;
                        new_cs = Selector(ctx.vm_pop16());
                        new_fl = Eflags::from_bits(
                            (ctx.eflags.bits() & 0xffff_0000) | ctx.vm_pop16() as usize,
                        );
                    }
                    ctx.set_cs(new_cs);
                    ctx.set_vm_eflags(new_fl);
                    return true;
                }
                0xF4 => {
                    // F4: HLT
                    Hal::cpu().enable_interrupt();
                    Hal::cpu().wait_for_interrupt();
                    skip += 1;
                }
                0xFA => {
                    // FA: CLI
                    ctx.eflags.remove(Eflags::IF);
                    skip += 1;
                }
                0xFB => {
                    // FB: STI
                    ctx.eflags.insert(Eflags::IF);
                    skip += 1;
                }
                _ => {
                    // Unsupported
                    return false;
                }
            }

            ctx.eip = ctx.eip.wrapping_add(skip as u32) & 0x0000_ffff;
            return true;
        }
    }
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

    #[inline]
    pub fn vm_eflags(&self) -> Eflags {
        let mut eflags = self.eflags.canonicalized();
        eflags.remove(Eflags::VM);
        eflags.clear_iopl();
        eflags
    }

    #[inline]
    pub fn set_vm_eflags(&mut self, eflags: Eflags) {
        let mut eflags = eflags.canonicalized();
        eflags.insert(Eflags::VM);
        eflags.set_iopl(VM86::IOPL_VM);
        self.eflags = eflags;
    }

    #[inline]
    pub fn adjust_vm_eflags(&mut self) {
        self.set_vm_eflags(self.eflags);
    }

    #[inline]
    pub fn vm_csip_ptr(&self) -> *mut u8 {
        Far16Ptr::new(self.cs(), Offset16::new(self.eip as u16)).to_ptr()
    }

    #[inline]
    pub unsafe fn vm_sssp_ptr(&self) -> *mut u16 {
        let (ss, esp) = unsafe { self.ss_esp3_unchecked() };
        Far16Ptr::new(ss, Offset16::new(esp as u16)).to_ptr()
    }

    #[inline]
    pub unsafe fn vm_sssp_ptr32(&self) -> *mut u32 {
        let (ss, esp) = unsafe { self.ss_esp3_unchecked() };
        Far16Ptr::new(ss, Offset16::new(esp as u16)).to_ptr()
    }

    #[inline]
    pub unsafe fn vm_push16(&mut self, value: u16) {
        unsafe {
            self.fix_esp3(|esp3| esp3.wrapping_sub(size_of_val(&value) as u32) & 0x0000_ffff);
            Self::vm_sssp_ptr(self).write_volatile(value);
        }
    }

    #[inline]
    pub unsafe fn vm_pop16(&mut self) -> u16 {
        unsafe {
            let value = Self::vm_sssp_ptr(self).read_volatile();
            self.fix_esp3(|esp3| esp3.wrapping_add(size_of_val(&value) as u32) & 0x0000_ffff);
            value
        }
    }

    #[inline]
    pub unsafe fn vm_push32(&mut self, value: u32) {
        unsafe {
            self.fix_esp3(|esp3| esp3.wrapping_sub(size_of_val(&value) as u32) & 0x0000_ffff);
            Self::vm_sssp_ptr32(self).write_volatile(value);
        }
    }

    #[inline]
    pub unsafe fn vm_pop32(&mut self) -> u32 {
        unsafe {
            let value = Self::vm_sssp_ptr32(self).read_volatile();
            self.fix_esp3(|esp3| esp3.wrapping_add(size_of_val(&value) as u32) & 0x0000_ffff);
            value
        }
    }
}

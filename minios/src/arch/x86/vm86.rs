//! Simple Virtual 8086 Mode Manager

use super::cpu::Cpu;
use super::gdt::Gdt;
use super::idt::Idt;
use super::lomem::{LoMemoryManager, ManagedLowMemory};
use super::setjmp::JmpBuf;
use crate::*;
use core::cell::UnsafeCell;
use core::num::NonZeroUsize;
use core::ptr::null_mut;
use x86::{gpr::*, prot::*, real::*};

static mut VMM: UnsafeCell<VM86> = UnsafeCell::new(VM86::new());

/// Simple Virtual 8086 Mode Manager
pub struct VM86 {
    vmbp: Linear32,
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
            Self::invoke(ctx, |ctx| {
                ctx.vm_redirect_interrupt(int_vec, true);
            });
        }
    }

    /// Invokes virtual 8086 mode with FAR CALL instruction executed.
    ///
    #[allow(unused)]
    pub unsafe fn call_far(target: Far16Ptr, ctx: &mut X86StackContext) {
        unsafe {
            Self::invoke(ctx, |ctx| {
                ctx.vm_call_far(target);
            });
        }
    }

    /// Invokes virtual 8086 mode.
    #[inline(always)]
    pub unsafe fn invoke<F>(ctx: &mut X86StackContext, modifier: F)
    where
        F: FnOnce(&mut X86StackContext),
    {
        without_interrupts!(unsafe {
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
            ctx.set_esp3(Pointer32::from_u16(vm_stack.limit().as_u16() & 0xfffe));
            let vmbp_csip = Far16Ptr::from_linear(shared.vmbp);
            ctx.set_cs(vmbp_csip.sel());
            ctx.eip = Pointer32::from(vmbp_csip.off());
            modifier(ctx);

            shared.context = ctx;
            if shared.jmp_buf.set_jmp().is_returned() {
                Cpu::enter_to_user_mode(ctx);
            }
            drop(temp_stack);

            Gdt::set_tss_esp0(old_tss_esp0);
            shared.jmp_buf = old_jmp_buf;
            shared.context = old_context;
            shared.vm_stack = old_vm_stack;
        });
    }

    #[inline(always)]
    unsafe fn intercept_vmbp(&mut self, ctx: &X86StackContext) -> ! {
        unsafe {
            self.context.write_volatile(ctx.clone());
            self.jmp_buf.long_jmp(NonZeroUsize::new(1).unwrap());
        }
    }

    /// Handles #UD exception in virtual 8086 mode.
    unsafe fn _handle_ud(ctx: &mut X86StackContext) -> bool {
        unsafe {
            if ctx.is_vm() {
                let vm_csip = Far16Ptr::new(ctx.cs(), Offset16::new(ctx.eip.as_u16())).to_linear();
                let shared = Self::shared_mut();
                if shared.vmbp == vm_csip {
                    shared.intercept_vmbp(ctx);
                }
            }
        }
        return false;
    }

    /// Handles #GP exception in virtual 8086 mode.
    unsafe fn _handle_gpf(ctx: &mut X86StackContext) -> bool {
        unsafe {
            if ctx.is_vm() {
                let vm_csip = Far16Ptr::new(ctx.cs(), Offset16::new(ctx.eip.as_u16())).to_linear();
                let shared = Self::shared_mut();
                if shared.vmbp == vm_csip {
                    shared.intercept_vmbp(ctx);
                }

                let err = ctx.selector_error_code();
                if let Some(int_vec) = err.int_vec() {
                    ctx.vm_redirect_interrupt(int_vec, false);
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
                ctx.vm_redirect_interrupt(int_vec, true);
            } else {
                let mut regs = X86StackContext::default();
                Self::call_bios(int_vec, &mut regs);
            }
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
                    ctx.vm_redirect_interrupt(int_vec, false);
                    return true;
                }
                0xCC => {
                    // CC: INT3
                    ctx.vm_redirect_interrupt(Exception::Breakpoint.as_vec(), false);
                    return true;
                }
                0xCF => {
                    // CF: IRET
                    let new_cs: Selector;
                    let new_fl: Eflags;
                    let new_eip: Pointer32;
                    if prefix_66 {
                        new_eip = Pointer32(ctx.vm_pop32());
                        new_cs = Selector(ctx.vm_pop32() as u16);
                        new_fl = Eflags::from_bits(ctx.vm_pop32() as usize);
                    } else {
                        new_eip = Pointer32::from_u16(ctx.vm_pop16());
                        new_cs = Selector(ctx.vm_pop16());
                        new_fl = Eflags::from_bits(
                            (ctx.eflags.bits() & 0xffff_0000) | ctx.vm_pop16() as usize,
                        );
                    }
                    ctx.set_cs(new_cs);
                    ctx.eip = new_eip;
                    ctx.set_vm_eflags(new_fl);
                    return true;
                }
                0xF4 => {
                    // F4: HLT
                    Hal::cpu().enable_interrupt();
                    Hal::cpu().wait_for_interrupt();
                    Hal::cpu().disable_interrupt();
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

            ctx.eip = Pointer32::from(ctx.eip.as_u16().wrapping_add(skip as u16));
            return true;
        }
    }
}

/// A stack structure for handling exceptions and entering virtual 8086 mode.
#[allow(dead_code)]
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct X86StackContext {
    _es: AlignedSelector32,
    _ds: AlignedSelector32,
    _fs: AlignedSelector32,
    _gs: AlignedSelector32,

    // same layout as `PUSHAD`
    pub edi: Gpr32,
    pub esi: Gpr32,
    pub ebp: Gpr32,
    // dummy for esp
    _esp: Gpr32,
    pub ebx: Gpr32,
    pub edx: Gpr32,
    pub ecx: Gpr32,
    pub eax: Gpr32,

    // `vector` and `error_code` are set only when an exception occurs
    _vector: u32,
    _error_code: u32,

    pub eip: Pointer32,
    _cs: AlignedSelector32,
    pub eflags: Eflags,

    // Valid only if `is_user()` is `true` after this point.
    _esp3: Pointer32,
    _ss3: AlignedSelector32,

    // Valid only if `is_vm()` is `true` after this point.
    _vmes: AlignedSelector32,
    _vmds: AlignedSelector32,
    _vmfs: AlignedSelector32,
    _vmgs: AlignedSelector32,
}

#[allow(unused)]
impl X86StackContext {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            _es: AlignedSelector32(0),
            _ds: AlignedSelector32(0),
            _fs: AlignedSelector32(0),
            _gs: AlignedSelector32(0),
            edi: Gpr32(0),
            esi: Gpr32(0),
            ebp: Gpr32(0),
            _esp: Gpr32(0),
            ebx: Gpr32(0),
            edx: Gpr32(0),
            ecx: Gpr32(0),
            eax: Gpr32(0),
            _vector: 0,
            _error_code: 0,
            eip: Pointer32(0),
            _cs: AlignedSelector32(0),
            eflags: Eflags::empty(),
            _esp3: Pointer32(0),
            _ss3: AlignedSelector32(0),
            _vmes: AlignedSelector32(0),
            _vmds: AlignedSelector32(0),
            _vmfs: AlignedSelector32(0),
            _vmgs: AlignedSelector32(0),
        }
    }

    /// Returns `true` if the context is in virtual 8086 mode.
    #[inline]
    pub fn is_vm(&self) -> bool {
        self.eflags.contains(Eflags::VM)
    }

    /// Returns `true` if the context is in user mode or virtual 8086 mode.
    #[inline]
    pub fn is_user(&self) -> bool {
        self.is_vm() || (self.cs().rpl() != RPL0)
    }

    #[inline]
    pub const fn cs(&self) -> Selector {
        self._cs.sel()
    }

    #[inline]
    pub const fn ds(&self) -> Selector {
        self._ds.sel()
    }

    #[inline]
    pub const fn es(&self) -> Selector {
        self._es.sel()
    }

    #[inline]
    pub const fn fs(&self) -> Selector {
        self._fs.sel()
    }

    #[inline]
    pub const fn gs(&self) -> Selector {
        self._gs.sel()
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

    /// Returns the value of DS in virtual 8086 mode.
    #[inline]
    pub fn vmds(&self) -> Option<Selector> {
        if self.is_vm() {
            Some(self._vmds.sel())
        } else {
            None
        }
    }

    /// Returns the value of ES in virtual 8086 mode.
    #[inline]
    pub fn vmes(&self) -> Option<Selector> {
        if self.is_vm() {
            Some(self._vmes.sel())
        } else {
            None
        }
    }

    /// Returns the value of FS in virtual 8086 mode.
    #[inline]
    pub fn vmfs(&self) -> Option<Selector> {
        if self.is_vm() {
            Some(self._vmfs.sel())
        } else {
            None
        }
    }

    /// Returns the value of GS in virtual 8086 mode.
    #[inline]
    pub fn vmgs(&self) -> Option<Selector> {
        if self.is_vm() {
            Some(self._vmgs.sel())
        } else {
            None
        }
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn vmds_unchecked(&self) -> Selector {
        self._vmds.sel()
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn vmes_unchecked(&self) -> Selector {
        self._vmes.sel()
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn vmfs_unchecked(&self) -> Selector {
        self._vmfs.sel()
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn vmgs_unchecked(&self) -> Selector {
        self._vmgs.sel()
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn set_vmds(&mut self, vmds: Selector) {
        self._vmds = AlignedSelector32::from(vmds);
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn set_vmes(&mut self, vmes: Selector) {
        self._vmes = AlignedSelector32::from(vmes);
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn set_vmfs(&mut self, vmfs: Selector) {
        self._vmfs = AlignedSelector32::from(vmfs);
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn set_vmgs(&mut self, vmgs: Selector) {
        self._vmgs = AlignedSelector32::from(vmgs);
    }

    #[inline]
    pub fn set_cs(&mut self, cs: Selector) {
        self._cs = AlignedSelector32::from(cs);
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in user mode or virtual 8086 mode.
    #[inline]
    pub unsafe fn set_esp3(&mut self, esp3: Pointer32) {
        self._esp3 = esp3;
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in user mode or virtual 8086 mode.
    #[inline]
    pub unsafe fn esp3_unchecked(&self) -> Pointer32 {
        self._esp3
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in user mode or virtual 8086 mode.
    #[inline]
    pub unsafe fn update_esp3<F>(&mut self, f: F)
    where
        F: FnOnce(Pointer32) -> Pointer32,
    {
        self._esp3 = f(self._esp3);
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in user mode or virtual 8086 mode.
    #[inline]
    pub unsafe fn update_sp3<F>(&mut self, f: F)
    where
        F: FnOnce(u16) -> u16,
    {
        self._esp3 = Pointer32::from_u16(f(self._esp3.as_u16()));
    }

    #[inline]
    pub fn esp3(&self) -> Option<Pointer32> {
        if self.is_user() {
            Some(self._esp3)
        } else {
            None
        }
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in user mode or virtual 8086 mode.
    #[inline]
    pub unsafe fn ss3_unchecked(&self) -> Selector {
        self._ss3.sel()
    }

    #[inline]
    pub fn ss3(&self) -> Option<Selector> {
        if self.is_user() {
            Some(self._ss3.sel())
        } else {
            None
        }
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in user mode or virtual 8086 mode.
    #[inline]
    pub unsafe fn set_ss3(&mut self, ss3: Selector) {
        self._ss3 = AlignedSelector32::from(ss3);
    }

    #[inline]
    pub fn ss_esp3(&self) -> Option<(Selector, Pointer32)> {
        if self.is_user() {
            Some((self._ss3.sel(), self._esp3))
        } else {
            None
        }
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in user mode or virtual 8086 mode.
    #[inline]
    pub unsafe fn ss_esp3_unchecked(&self) -> (Selector, Pointer32) {
        (self._ss3.sel(), self._esp3)
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
        Far16Ptr::new(self.cs(), self.eip.offset16()).as_ptr()
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn vm_sssp_ptr16(&self) -> *mut u16 {
        let (ss, esp) = unsafe { self.ss_esp3_unchecked() };
        Far16Ptr::new(ss, esp.offset16()).as_ptr()
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn vm_sssp_ptr32(&self) -> *mut u32 {
        let (ss, esp) = unsafe { self.ss_esp3_unchecked() };
        Far16Ptr::new(ss, esp.offset16()).as_ptr()
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn vm_push16(&mut self, value: u16) {
        unsafe {
            self.update_sp3(|sp3| sp3.wrapping_sub(size_of_val(&value) as u16));
            self.vm_sssp_ptr16().write_volatile(value);
        }
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn vm_pop16(&mut self) -> u16 {
        unsafe {
            let value = self.vm_sssp_ptr16().read_volatile();
            self.update_sp3(|sp3| sp3.wrapping_add(size_of_val(&value) as u16));
            value
        }
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn vm_push32(&mut self, value: u32) {
        unsafe {
            self.update_sp3(|sp3| sp3.wrapping_sub(size_of_val(&value) as u16));
            self.vm_sssp_ptr32().write_volatile(value);
        }
    }

    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn vm_pop32(&mut self) -> u32 {
        unsafe {
            let value = self.vm_sssp_ptr32().read_volatile();
            self.update_sp3(|sp3| sp3.wrapping_add(size_of_val(&value) as u16));
            value
        }
    }

    /// Simulates a far call in virtual 8086 mode.
    ///
    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    pub unsafe fn vm_call_far(&mut self, target: Far16Ptr) {
        unsafe {
            self.vm_push16(self.cs().as_u16());
            self.vm_push16(self.eip.offset16().as_u16());
            self.set_cs(target.sel());
            self.eip = Pointer32::from(target.off());
        }
    }

    /// Redirect interrupts by adjusting the stack context in virtual 8086 mode.
    ///
    /// # Safety
    ///
    /// Caller must ensure that the context is in virtual 8086 mode.
    #[inline]
    unsafe fn vm_redirect_interrupt(&mut self, int_vec: InterruptVector, is_external: bool) {
        unsafe {
            if is_external || self.selector_error_code().is_external() {
                self.vm_push16(self.eflags.bits() as u16);
                self.vm_push16(self.cs().as_u16());
                self.vm_push16(self.eip.as_u16());
            } else {
                let mut skip = 0;
                let vm_csip = self.vm_csip_ptr();
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
                self.vm_push16(self.eflags.bits() as u16);
                self.vm_push16(self.cs().as_u16());
                self.vm_push16((self.eip.as_u16()).wrapping_add(skip));
            }

            let vm_intvec =
                Far16Ptr::from_u32((((int_vec.0 as usize) << 2) as *const u32).read_volatile());
            self.eip = Pointer32::from(vm_intvec.off());
            self.set_cs(vm_intvec.sel());
        }
    }
}

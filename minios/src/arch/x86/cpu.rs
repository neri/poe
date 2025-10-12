//! i386 cpu core logic

use super::vm86::X86StackContext;
use core::arch::{asm, naked_asm};
use core::mem::size_of;
use core::sync::atomic::{Ordering, compiler_fence};
use x86::prot::*;

pub struct Cpu {}

impl Cpu {
    #[inline]
    pub(crate) unsafe fn init() {
        unsafe {
            super::gdt::Gdt::init();
            super::idt::Idt::init();
        }
    }

    /// Enter to user mode with specified stack context
    #[inline(always)]
    pub unsafe fn enter_to_user_mode(regs: &X86StackContext) -> ! {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            Self::_iret_to_user_mode(regs, super::gdt::Gdt::shared().tss_mut());
        }
    }

    #[unsafe(naked)]
    unsafe extern "fastcall" fn _iret_to_user_mode(
        regs: &X86StackContext,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SetDescriptorError {
    OutOfIndex,
    PriviledgeMismatch,
}

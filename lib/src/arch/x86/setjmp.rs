//! setjmp/longjmp

use core::{
    arch::naked_asm,
    num::NonZeroUsize,
    sync::atomic::{Ordering, compiler_fence},
};

#[derive(Default, Clone)]
#[allow(unused)]
pub struct JmpBuf([usize; 8]);

impl JmpBuf {
    #[inline]
    pub const fn new() -> Self {
        Self([0; 8])
    }

    #[inline]
    pub unsafe fn set_jmp(&mut self) -> Option<NonZeroUsize> {
        compiler_fence(Ordering::SeqCst);
        unsafe { Self::_set_jmp(self) }
    }

    #[inline]
    pub unsafe fn long_jmp(&mut self, value: NonZeroUsize) -> ! {
        compiler_fence(Ordering::SeqCst);
        unsafe { Self::_long_jmp(self, value) }
    }

    #[unsafe(naked)]
    unsafe extern "fastcall" fn _set_jmp(buf: &mut Self) -> Option<NonZeroUsize> {
        naked_asm!(
            "mov [ecx], esp",
            "mov [ecx + 4], ebp",
            "mov [ecx + 8], ebx",
            "mov [ecx + 12], esi",
            "mov [ecx + 16], edi",
            "mov eax, [esp]",
            "mov [ecx + 20], eax",
            "xor eax, eax",
            "ret",
        )
    }

    #[unsafe(naked)]
    unsafe extern "fastcall" fn _long_jmp(buf: &mut Self, value: NonZeroUsize) -> ! {
        naked_asm!(
            "mov eax, edx",
            "mov esp, [ecx]",
            "mov ebp, [ecx + 4]",
            "mov ebx, [ecx + 8]",
            "mov esi, [ecx + 12]",
            "mov edi, [ecx + 16]",
            "mov edx, [ecx + 20]",
            "mov [esp], edx",
            "ret",
        )
    }
}

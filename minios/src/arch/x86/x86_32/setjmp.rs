//! setjmp/longjmp

use alloc::rc::Rc;
use core::{
    arch::naked_asm,
    marker::PhantomData,
    num::NonZero,
    sync::atomic::{Ordering, compiler_fence},
};

#[allow(unused)]
#[derive(Default)]
pub struct JmpBuf {
    data: [usize; 8],

    // To prevent `Send` and `Sync` auto traits
    _phantom: PhantomData<Rc<()>>,
}

impl JmpBuf {
    #[inline]
    pub const fn zeroed() -> Self {
        Self {
            data: [0; 8],
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub unsafe fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub unsafe fn set_jmp(&mut self) -> SetJmpResult {
        compiler_fence(Ordering::SeqCst);
        let result = unsafe { Self::_set_jmp(self) };
        compiler_fence(Ordering::SeqCst);
        result
    }

    #[inline]
    pub unsafe fn long_jmp(&mut self, value: NonZero<usize>) -> ! {
        compiler_fence(Ordering::SeqCst);
        unsafe { Self::_long_jmp(self, value) }
    }

    #[unsafe(naked)]
    unsafe extern "fastcall" fn _set_jmp(buf: &mut Self) -> SetJmpResult {
        naked_asm!(
            "mov [ecx], esp",
            "mov [ecx + 4], ebp",
            "mov [ecx + 8], ebx",
            "mov [ecx + 12], esi",
            "mov [ecx + 16], edi",
            "mov edx, [esp]",
            "mov [ecx + 20], edx",
            "xor eax, eax",
            "ret",
        )
    }

    #[unsafe(naked)]
    unsafe extern "fastcall" fn _long_jmp(buf: &mut Self, value: NonZero<usize>) -> ! {
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

#[must_use]
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetJmpResult {
    Returned,
    LongJumped(NonZero<usize>),
}

#[allow(dead_code)]
impl SetJmpResult {
    #[inline]
    pub const fn is_returned(&self) -> bool {
        matches!(self, Self::Returned)
    }

    #[inline]
    pub const fn is_long_jumped(&self) -> bool {
        matches!(self, Self::LongJumped(_))
    }

    #[inline]
    pub const fn long_jumped(self) -> Option<NonZero<usize>> {
        match self {
            Self::Returned => None,
            Self::LongJumped(v) => Some(v),
        }
    }
}

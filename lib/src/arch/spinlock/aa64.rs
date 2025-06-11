//! Spinlock for aa64
use core::{arch::asm, sync::atomic::AtomicU32};

pub struct Spinlock {
    value: AtomicU32,
}

impl Spinlock {
    pub const LOCKED_VALUE: u32 = 1;
    pub const UNLOCKED_VALUE: u32 = 0;

    #[inline]
    pub const fn new() -> Self {
        Self {
            value: AtomicU32::new(Self::UNLOCKED_VALUE),
        }
    }

    #[must_use]
    pub fn try_lock(&self) -> bool {
        let result: u32;
        unsafe {
            asm!("
                    ldaxr {0:w}, [{1}]
                    cbnz {0:w}, 1f
                    stxr {0:w}, {2:w}, [{1}]
                1:
                ", out(reg)result, in(reg)&self.value, in(reg)Self::LOCKED_VALUE);
        }
        result == 0
    }

    pub fn lock(&self) {
        unsafe {
            asm!("
                    sevl
                1:  wfe
                2:  ldaxr {0:w}, [{1}]
                    cbnz {0:w}, 1b
                    stxr {0:w}, {2:w}, [{1}]
                    cbnz {0:w}, 2b
                ", out(reg)_, in(reg)&self.value, in(reg)Self::LOCKED_VALUE);
        }
    }

    #[inline]
    pub unsafe fn force_unlock(&self) {
        unsafe {
            asm!("
                    stlr {1:w}, [{0}]
                ", in(reg)&self.value, in(reg)Self::UNLOCKED_VALUE);
        }
    }
}

//! Spinlock
use core::sync::atomic::{AtomicU32, Ordering};

pub struct Spinlock {
    value: AtomicU32,
}

impl Spinlock {
    const LOCKED_VALUE: u32 = 1;
    const UNLOCKED_VALUE: u32 = 0;

    #[inline]
    pub const fn new() -> Self {
        Self {
            value: AtomicU32::new(Self::UNLOCKED_VALUE),
        }
    }

    #[inline]
    #[must_use]
    pub fn try_lock(&self) -> bool {
        self.value.load(Ordering::Relaxed) == Self::UNLOCKED_VALUE
            && self.value.swap(Self::LOCKED_VALUE, Ordering::Acquire) == Self::UNLOCKED_VALUE
    }

    #[inline]
    pub fn lock(&self) {
        while !self.try_lock() {}
    }

    #[inline]
    pub unsafe fn force_unlock(&self) {
        self.value.swap(Self::UNLOCKED_VALUE, Ordering::Release);
    }
}

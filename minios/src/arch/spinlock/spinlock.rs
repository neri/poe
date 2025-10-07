//! Spinlock
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct Spinlock {
    value: AtomicUsize,
}

impl Spinlock {
    const LOCKED_VALUE: usize = 1;
    const UNLOCKED_VALUE: usize = 0;

    #[inline]
    pub const fn new() -> Self {
        Self {
            value: AtomicUsize::new(Self::UNLOCKED_VALUE),
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

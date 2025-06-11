//! Spinlock for x86-64
use core::{
    arch::asm,
    sync::atomic::{AtomicBool, Ordering},
};

pub struct Spinlock {
    value: AtomicBool,
}

impl Spinlock {
    const LOCKED_VALUE: bool = true;
    const UNLOCKED_VALUE: bool = false;

    #[inline]
    pub const fn new() -> Self {
        Self {
            value: AtomicBool::new(Self::UNLOCKED_VALUE),
        }
    }

    #[inline]
    #[must_use]
    pub fn try_lock(&self) -> bool {
        self.value
            .compare_exchange(
                Self::UNLOCKED_VALUE,
                Self::LOCKED_VALUE,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
    }

    pub fn lock(&self) {
        while self
            .value
            .compare_exchange(
                Self::UNLOCKED_VALUE,
                Self::LOCKED_VALUE,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_err()
        {
            let mut spin_loop = SpinLoopWait::new();
            while self.value.load(Ordering::Acquire) {
                spin_loop.wait();
            }
        }
    }

    #[inline]
    pub unsafe fn force_unlock(&self) -> Option<()> {
        self.value
            .compare_exchange(
                Self::LOCKED_VALUE,
                Self::UNLOCKED_VALUE,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .map(|_| ())
            .ok()
    }
}

struct SpinLoopWait(usize);

impl SpinLoopWait {
    #[inline]
    pub const fn new() -> Self {
        Self(0)
    }

    #[inline]
    fn wait(&mut self) {
        let count = self.0;
        for _ in 0..(1 << count) {
            unsafe {
                asm!("pause", options(nomem, nostack));
            }
        }
        if count < 6 {
            self.0 += 1;
        }
    }
}

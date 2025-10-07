//! Spinlock for x86
use core::{arch::asm, mem::transmute, sync::atomic::AtomicU8};

pub struct Spinlock {
    value: AtomicU8,
}

impl Spinlock {
    const UNLOCKED_VALUE: u8 = false as u8;

    const LOCKED_VALUE: u8 = true as u8;

    #[inline]
    pub const fn new() -> Self {
        Self {
            value: AtomicU8::new(Self::UNLOCKED_VALUE),
        }
    }

    #[inline]
    #[must_use]
    pub fn try_lock(&self) -> bool {
        // Do not use cmpxchg instruction to run on i386
        unsafe {
            let result: u8;
            asm!(
                "xchg [{}], {}",
                in(reg) &self.value,
                inout(reg_byte) Self::LOCKED_VALUE => result,
            );
            transmute(result)
        }
    }

    #[inline]
    pub fn lock(&self) {
        while !self.try_lock() {
            unsafe {
                asm!("pause", ".byte 0xeb, 0x00", options(nomem, nostack));
            }
        }
    }

    #[inline]
    pub unsafe fn force_unlock(&self) -> Option<()> {
        unsafe {
            let result: u8;
            asm!(
                "xchg [{}], {}",
                in(reg) &self.value,
                inout(reg_byte) Self::UNLOCKED_VALUE => result,
            );
            (transmute::<_, bool>(result)).then(|| ())
        }
    }
}

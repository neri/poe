//! Orders DMA memory and device MMIO operations on the supported platforms.
use core::sync::atomic::{Ordering, fence};

#[inline]
pub fn device() {
    fence(Ordering::SeqCst);
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    unsafe {
        core::arch::asm!("fence iorw, iorw", options(nostack))
    };
    #[cfg(all(target_arch = "aarch64", not(test)))]
    unsafe {
        core::arch::asm!("dmb osh", options(nostack))
    };
}

//! Cache maintenance for AArch64

use core::arch::asm;

/// Cleans and invalidates the data cache for the range, to the point of coherency.
///
/// This also works with the MMU and the data cache off (VA = PA).
/// Use it before reading data that the previous boot stage may have left only in the cache.
pub unsafe fn dcache_clean_invalidate(start: usize, size: usize) {
    if size == 0 {
        return;
    }
    let ctr: u64;
    unsafe {
        asm!("mrs {}, ctr_el0", out(reg) ctr);
    }
    // CTR_EL0.DminLine: log2 of the number of words in the smallest data cache line
    let line = 4usize << ((ctr >> 16) & 0xf);
    let end = start.saturating_add(size);
    let mut p = start & !(line - 1);
    while p < end {
        unsafe {
            asm!("dc civac, {}", in(reg) p);
        }
        p += line;
    }
    unsafe {
        asm!("dsb sy", "isb");
    }
}

/// Cleans a range before transferring ownership to a DMA reader.
pub unsafe fn dcache_clean(start: usize, size: usize) {
    unsafe { dcache_range(start, size, "cvac") }
}

/// Invalidates a range after a DMA writer has returned ownership to the CPU.
pub unsafe fn dcache_invalidate(start: usize, size: usize) {
    unsafe { dcache_range(start, size, "ivac") }
}

#[inline]
unsafe fn dcache_range(start: usize, size: usize, operation: &str) {
    if size == 0 {
        return;
    }
    let ctr: u64;
    unsafe {
        asm!("mrs {}, ctr_el0", out(reg) ctr);
    }
    let line = 4usize << ((ctr >> 16) & 0xf);
    let end = start.saturating_add(size);
    let mut p = start & !(line - 1);
    while p < end {
        unsafe {
            match operation {
                "cvac" => asm!("dc cvac, {}", in(reg) p),
                _ => asm!("dc ivac, {}", in(reg) p),
            }
        }
        p += line;
    }
    unsafe {
        asm!("dsb sy", "isb");
    }
}

//! Data cache maintenance for the T-Head C906 (XTheadCmo).
//!
//! The C906 has no cache-coherent DMA. The `th.dcache.*` instructions are
//! custom-0 encodings, written out as words so no assembler extension is
//! needed; the address goes in `a0`. S-mode may execute them once the SBI
//! firmware has set `mxstatus.THEADISAEE`, as the vendor OpenSBI does for
//! Linux. With translation off, the virtual address forms act on physical
//! addresses.

const CACHE_LINE: usize = 64;

/// `th.dcache.cva a0`: write back the line.
const CLEAN_A0: u32 = 0x0255_000b;
/// `th.dcache.iva a0`: discard the line.
const INVALIDATE_A0: u32 = 0x0265_000b;
/// `th.dcache.civa a0`: write back, then discard the line.
const CLEAN_INVALIDATE_A0: u32 = 0x0275_000b;

macro_rules! for_each_line {
    ($start:expr, $size:expr, $insn:expr) => {{
        let end = $start.saturating_add($size);
        let mut line = $start & !(CACHE_LINE - 1);
        while line < end {
            unsafe { core::arch::asm!(".word {insn}", insn = const $insn, in("a0") line, options(nostack)) };
            line += CACHE_LINE;
        }
        // `th.sync.s`: wait until the cache operations have completed.
        unsafe { core::arch::asm!(".word 0x0190000b", options(nostack)) };
    }};
}

pub unsafe fn dcache_clean(start: usize, size: usize) {
    for_each_line!(start, size, CLEAN_A0)
}

pub unsafe fn dcache_invalidate(start: usize, size: usize) {
    for_each_line!(start, size, INVALIDATE_A0)
}

pub unsafe fn dcache_clean_invalidate(start: usize, size: usize) {
    for_each_line!(start, size, CLEAN_INVALIDATE_A0)
}

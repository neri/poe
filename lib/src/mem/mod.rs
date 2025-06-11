pub mod global_alloc;
pub mod mmio;

mod mm;
pub use mm::*;

use core::{cmp, ops::Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    Used = 0,
    Available = 1,
    Reserved = 2,
    AcpiReclaim = 3,
    AcpiNvs = 4,
    DeviceTree = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAllocationStrategy {
    FirstFit,
    BestFit,
    LastFit,
}

#[repr(C)]
#[derive(Clone, PartialEq, Eq)]
pub struct MemoryMapEntry {
    pub base: u64,
    pub size: u64,
    pub mem_type: MemoryType,
}

impl MemoryMapEntry {
    #[inline]
    pub const fn new(base: u64, size: u64, mem_type: MemoryType) -> Self {
        Self {
            base,
            size,
            mem_type,
        }
    }

    #[inline]
    pub fn range(&self) -> Range<u64> {
        Range {
            start: self.base,
            end: self.base + self.size,
        }
    }
}

impl PartialOrd for MemoryMapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.base.partial_cmp(&other.base)
    }
}

impl Ord for MemoryMapEntry {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.base.cmp(&other.base)
    }
}

impl core::fmt::Display for MemoryMapEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let range = self.range();
        write!(
            f,
            "{:016x}-{:016x}: {:?}",
            range.start,
            range.end - 1,
            self.mem_type
        )
    }
}

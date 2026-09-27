#[cfg(not(feature = "uefi"))]
pub mod global_alloc;

pub mod mmio;

#[cfg(feature = "device_tree")]
pub mod dt;
mod mm;
pub mod region;
use core::cmp;
use core::ops::Range;

pub use mm::*;
pub use region::{
    ALLOCATION_LIMIT, AllocRequest, MapError, MemoryError, MemoryFreeError, MemoryRegion,
    PhysRange, RegionAttrs,
};

/// Use of a physical memory region
///
/// The values are internal to MiniOS; the map handed to an OS is converted to
/// the representation of its boot protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    /// MiniOS itself: image, stacks, heap, the map table. Reclaimable once
    /// control has passed to the next OS.
    Loader = 0,
    Available = 1,
    /// Reserved, including reservations of unknown purpose
    Reserved = 2,
    AcpiReclaim = 3,
    AcpiNvs = 4,
    /// The device tree blob received at boot, handed over as it is
    DeviceTree = 5,
    /// Other firmware reserved area
    OtherFw = 6,
    /// The kernel of the next OS
    Kernel = 7,
    /// Memory map, command line and other boot information for the next OS
    BootData = 8,
    /// Initial ramdisk for the next OS
    Initrd = 9,
    /// Memory needed to keep the display on across the handoff
    Framebuffer = 10,
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

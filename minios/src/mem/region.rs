//! Physical memory map
//!
//! Heap-free building blocks of the memory manager: 64-bit half-open ranges,
//! the normalization of RAM and protected inputs into a sorted list of regions
//! that do not overlap, and the table operations used by the allocator.
//! Everything here works on storage supplied by the caller, so it runs before
//! the heap exists and in host tests.
//!
//! Rules (see `docs/DEVICE_TREE_MEMORY_PLAN.md`):
//!
//! - RAM is rounded inward to pages, protected ranges outward.
//! - Every region is split at [`ALLOCATION_LIMIT`]; only regions below it are
//!   allocated from. RAM above it stays `Available` in the map.
//! - Overlaps that cannot both hold (the MiniOS image with anything else, two
//!   different live objects, an object in a firmware-only range) are rejected
//!   on the exact byte ranges. Sharing a page only because of rounding is not
//!   an error: the page takes the stronger protection of the two.
//! - An allocation keeps its own entry until it is freed; free entries are
//!   merged with their free neighbours.

use core::fmt;
use core::ptr::NonNull;

use minilib::fixedvec::FixedVec;

use super::MemoryType;

pub const PAGE_SIZE: u64 = 0x1000;
const PAGE_MASK: u64 = !(PAGE_SIZE - 1);

/// MiniOS allocates only below this address. RAM above it is kept in the map
/// and handed over, but never returned by the allocator.
pub const ALLOCATION_LIMIT: u64 = 0x1_0000_0000;

/// A physical address range `[start, end)`, never empty
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysRange {
    start: u64,
    end: u64,
}

impl PhysRange {
    /// Returns `None` if the range is empty or inverted.
    #[inline]
    pub const fn new(start: u64, end: u64) -> Option<Self> {
        if start < end {
            Some(Self { start, end })
        } else {
            None
        }
    }

    /// `Ok(None)` for a zero size, an error if the end does not fit in 64 bits.
    #[inline]
    pub fn from_base_size(base: u64, size: u64) -> Result<Option<Self>, MapError> {
        if size == 0 {
            return Ok(None);
        }
        let end = base.checked_add(size).ok_or(MapError::Overflow)?;
        Ok(Some(Self { start: base, end }))
    }

    /// `Ok(None)` for `start == end`, an error for `start > end`.
    #[inline]
    pub fn from_bounds(start: u64, end: u64) -> Result<Option<Self>, MapError> {
        if start > end {
            Err(MapError::InvalidRange)
        } else {
            Ok(Self::new(start, end))
        }
    }

    #[inline]
    pub const fn start(&self) -> u64 {
        self.start
    }

    #[inline]
    pub const fn end(&self) -> u64 {
        self.end
    }

    #[inline]
    pub const fn len(&self) -> u64 {
        self.end - self.start
    }

    /// The whole pages inside the range, if any
    #[inline]
    pub fn page_inner(&self) -> Option<Self> {
        let start = self.start.checked_add(PAGE_SIZE - 1)? & PAGE_MASK;
        Self::new(start, self.end & PAGE_MASK)
    }

    /// The pages that the range touches
    #[inline]
    pub fn page_outer(&self) -> Result<Self, MapError> {
        let end = self
            .end
            .checked_add(PAGE_SIZE - 1)
            .ok_or(MapError::Overflow)?
            & PAGE_MASK;
        Ok(Self {
            start: self.start & PAGE_MASK,
            end,
        })
    }

    #[inline]
    pub const fn contains(&self, address: u64) -> bool {
        self.start <= address && address < self.end
    }

    #[inline]
    pub const fn contains_range(&self, other: &Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    #[inline]
    pub const fn intersects(&self, other: &Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    #[inline]
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        Self::new(self.start.max(other.start), self.end.min(other.end))
    }
}

impl fmt::Debug for PhysRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}..{:#x}", self.start, self.end)
    }
}

impl fmt::Display for PhysRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:010x}-{:010x}", self.start, self.end - 1)
    }
}

bitflags::bitflags! {
    /// Where a region came from and how it has to be treated, kept apart from its use
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct RegionAttrs: u32 {
        /// Backed by system RAM
        const RAM = 1 << 0;
        /// `no-map`: not to be mapped as normal memory
        const NO_MAP = 1 << 1;
        /// `reusable`: the OS may reuse it as the reservation allows
        const REUSABLE = 1 << 2;
        /// From the memory reservation block of the DTB
        const MEMRESERVE = 1 << 8;
        /// From a fixed `reg` of a `/reserved-memory` child
        const RESERVED_MEMORY = 1 << 9;
        /// From a firmware memory map or a platform specific source
        const FIRMWARE = 1 << 10;
        /// A live allocation of MiniOS, freed only as a whole
        const ALLOCATION = 1 << 16;
    }
}

impl RegionAttrs {
    /// Attributes that describe the memory itself, carried into the handoff map
    pub const HANDOFF: Self = Self::RAM
        .union(Self::NO_MAP)
        .union(Self::REUSABLE)
        .union(Self::MEMRESERVE)
        .union(Self::RESERVED_MEMORY)
        .union(Self::FIRMWARE);
}

/// One entry of the memory map
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MemoryRegion {
    pub range: PhysRange,
    pub mem_type: MemoryType,
    pub attrs: RegionAttrs,
}

impl MemoryRegion {
    #[inline]
    pub const fn new(range: PhysRange, mem_type: MemoryType, attrs: RegionAttrs) -> Self {
        Self {
            range,
            mem_type,
            attrs,
        }
    }

    #[inline]
    pub fn is_free(&self) -> bool {
        self.mem_type == MemoryType::Available
    }

    #[inline]
    pub fn is_allocation(&self) -> bool {
        self.attrs.contains(RegionAttrs::ALLOCATION)
    }

    /// Whether `next` can be folded into `self` without losing information
    #[inline]
    fn can_merge(&self, next: &Self) -> bool {
        self.range.end == next.range.start
            && self.range.end != ALLOCATION_LIMIT
            && self.mem_type == next.mem_type
            && self.attrs == next.attrs
            && !self.is_allocation()
            && !next.is_allocation()
    }
}

impl fmt::Debug for MemoryRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} {:?} {:?}", self.range, self.mem_type, self.attrs)
    }
}

impl fmt::Display for MemoryRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:?}", self.range, self.mem_type)?;
        let extra = self.attrs - RegionAttrs::RAM - RegionAttrs::ALLOCATION;
        if !extra.is_empty() {
            f.write_str(" ")?;
            bitflags::parser::to_writer(&extra, &mut *f)?;
        }
        Ok(())
    }
}

/// Failures while building or changing the map
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    /// An address computation does not fit in 64 bits
    Overflow,
    /// The end of a range is before its start
    InvalidRange,
    /// A fixed-size table is full; nothing was changed
    CapacityExceeded,
    /// Two ranges overlap that cannot both be honoured
    Conflict {
        first: (PhysRange, MemoryType),
        second: (PhysRange, MemoryType),
    },
    /// The range overlaps a live allocation
    Allocated(PhysRange),
    /// A property of the device tree cannot be read
    Malformed(&'static str),
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow => f.write_str("address overflow"),
            Self::InvalidRange => f.write_str("inverted range"),
            Self::CapacityExceeded => f.write_str("memory map table is full"),
            Self::Conflict { first, second } => write!(
                f,
                "{} {:?} overlaps {} {:?}",
                first.0, first.1, second.0, second.1
            ),
            Self::Allocated(range) => write!(f, "{} is allocated", range),
            Self::Malformed(what) => write!(f, "malformed {}", what),
        }
    }
}

/// Failures of an allocation; the table is unchanged
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    InvalidParameter,
    OutOfMemory,
    /// The table has no room for the entries the split needs
    TableFull,
    /// The final map has been produced; the map no longer changes
    Frozen,
    /// The memory manager has not been initialized
    NotInitialized,
}

/// Failures of a free or a change of use; the table is unchanged
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryFreeError {
    InvalidParameter,
    /// No region contains the address
    InvalidPointer,
    DoubleFree,
    /// The address is inside an allocation, but the size or start differs
    SizeMismatch,
    /// The region was registered, not allocated
    NotAllocated,
    /// The allocation has a different use
    TypeMismatch,
    Frozen,
}

impl MemoryType {
    /// Uses that an allocation may have
    #[inline]
    pub const fn is_allocatable(self) -> bool {
        matches!(
            self,
            Self::Loader | Self::Kernel | Self::BootData | Self::Initrd | Self::Framebuffer
        )
    }

    /// A specific object that is handed over and must be kept for it
    #[inline]
    const fn is_object(self) -> bool {
        matches!(
            self,
            Self::Kernel | Self::BootData | Self::Initrd | Self::DeviceTree | Self::Framebuffer
        )
    }

    /// Owned by the firmware; nothing else can live there
    #[inline]
    const fn is_firmware_only(self) -> bool {
        matches!(self, Self::OtherFw | Self::AcpiNvs | Self::AcpiReclaim)
    }

    /// Which use wins when two protections share a page
    #[inline]
    const fn protection_rank(self) -> u8 {
        match self {
            Self::Available => 0,
            Self::Loader => 1,
            Self::AcpiReclaim => 2,
            Self::Reserved => 3,
            Self::BootData => 4,
            Self::Kernel => 5,
            Self::Initrd => 6,
            Self::DeviceTree => 7,
            Self::Framebuffer => 8,
            Self::OtherFw => 9,
            Self::AcpiNvs => 10,
        }
    }
}

/// Whether two protections on the same bytes contradict each other
fn is_conflict(a: (MemoryType, RegionAttrs), b: (MemoryType, RegionAttrs)) -> bool {
    use MemoryType::*;
    let (ta, tb) = (a.0, b.0);
    if ta == Available || tb == Available {
        return false;
    }
    if (a.1 | b.1).contains(RegionAttrs::NO_MAP | RegionAttrs::REUSABLE) {
        return true;
    }
    if ta == tb {
        return false;
    }
    if ta == Loader || tb == Loader {
        return true;
    }
    if ta.is_object() && tb.is_object() {
        return true;
    }
    (ta.is_object() && tb.is_firmware_only()) || (tb.is_object() && ta.is_firmware_only())
}

/// The use and attributes of a page covered by both protections
fn combine(
    a: (MemoryType, RegionAttrs),
    b: (MemoryType, RegionAttrs),
) -> (MemoryType, RegionAttrs) {
    let mem_type = if a.0.protection_rank() >= b.0.protection_rank() {
        a.0
    } else {
        b.0
    };
    (mem_type, a.1 | b.1)
}

/// A range that must not be allocated, as given (not rounded)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtectedInput {
    pub range: PhysRange,
    pub mem_type: MemoryType,
    pub attrs: RegionAttrs,
}

/// Collects RAM and protected ranges, then produces the normalized map.
///
/// `R` and `P` are the capacities for RAM and protected ranges. Running out
/// is an error, never a silent truncation.
pub struct MapBuilder<const R: usize, const P: usize> {
    ram: FixedVec<PhysRange, R>,
    protected: FixedVec<ProtectedInput, P>,
}

impl<const R: usize, const P: usize> MapBuilder<R, P> {
    #[inline]
    pub const fn new() -> Self {
        Self {
            ram: FixedVec::new(),
            protected: FixedVec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.ram.clear();
        self.protected.clear();
    }

    pub fn add_ram(&mut self, range: PhysRange) -> Result<(), MapError> {
        self.ram.push(range).map_err(|_| MapError::CapacityExceeded)
    }

    pub fn add_protected(
        &mut self,
        range: PhysRange,
        mem_type: MemoryType,
        attrs: RegionAttrs,
    ) -> Result<(), MapError> {
        if mem_type == MemoryType::Available || attrs.contains(RegionAttrs::ALLOCATION) {
            return Err(MapError::InvalidRange);
        }
        range.page_outer()?;
        self.protected
            .push(ProtectedInput {
                range,
                mem_type,
                attrs,
            })
            .map_err(|_| MapError::CapacityExceeded)
    }

    #[inline]
    pub fn ram(&self) -> &[PhysRange] {
        self.ram.as_slice()
    }

    #[inline]
    pub fn protected(&self) -> &[ProtectedInput] {
        self.protected.as_slice()
    }

    /// Rejects protected ranges whose exact bytes overlap and cannot both hold.
    pub fn check_conflicts(&self) -> Result<(), MapError> {
        let items = self.protected.as_slice();
        for (i, a) in items.iter().enumerate() {
            for b in &items[i + 1..] {
                if a.range.intersects(&b.range)
                    && is_conflict((a.mem_type, a.attrs), (b.mem_type, b.attrs))
                {
                    return Err(MapError::Conflict {
                        first: (a.range, a.mem_type),
                        second: (b.range, b.mem_type),
                    });
                }
            }
        }
        Ok(())
    }

    /// Writes the normalized map of the RAM to `out` and returns its length.
    ///
    /// The result is sorted, does not overlap, is split at
    /// [`ALLOCATION_LIMIT`], and covers exactly the whole pages of the RAM.
    /// Parts of protected ranges outside the RAM are not added; they stay in
    /// [`Self::protected`].
    pub fn build(&self, out: &mut [MemoryRegion]) -> Result<usize, MapError> {
        self.check_conflicts()?;
        let mut len = 0;
        let mut cursor = 0;
        while let Some(next) = self.next_boundary(cursor) {
            if self.is_ram(cursor) {
                let piece = PhysRange {
                    start: cursor,
                    end: next,
                };
                let (mem_type, attrs) = self.classify(cursor);
                push_merged(
                    out,
                    &mut len,
                    MemoryRegion::new(piece, mem_type, attrs | RegionAttrs::RAM),
                )?;
            }
            cursor = next;
        }
        Ok(len)
    }

    fn next_boundary(&self, after: u64) -> Option<u64> {
        let mut next = (ALLOCATION_LIMIT > after).then_some(ALLOCATION_LIMIT);
        let mut consider = |value: u64| {
            if value > after && next.is_none_or(|v| value < v) {
                next = Some(value);
            }
        };
        for ram in self.ram.iter().filter_map(|v| v.page_inner()) {
            consider(ram.start);
            consider(ram.end);
        }
        for item in self.protected.iter() {
            // Checked when added
            if let Ok(outer) = item.range.page_outer() {
                consider(outer.start);
                consider(outer.end);
            }
        }
        next
    }

    fn is_ram(&self, address: u64) -> bool {
        self.ram
            .iter()
            .filter_map(|v| v.page_inner())
            .any(|v| v.contains(address))
    }

    fn classify(&self, address: u64) -> (MemoryType, RegionAttrs) {
        self.protected
            .iter()
            .filter(|v| v.range.page_outer().is_ok_and(|r| r.contains(address)))
            .fold((MemoryType::Available, RegionAttrs::empty()), |acc, v| {
                combine(acc, (v.mem_type, v.attrs))
            })
    }
}

fn push_merged(
    out: &mut [MemoryRegion],
    len: &mut usize,
    region: MemoryRegion,
) -> Result<(), MapError> {
    if let Some(last) = len.checked_sub(1).and_then(|i| out.get_mut(i))
        && last.can_merge(&region)
    {
        last.range.end = region.range.end;
        return Ok(());
    }
    let slot = out.get_mut(*len).ok_or(MapError::CapacityExceeded)?;
    *slot = region;
    *len += 1;
    Ok(())
}

/// Constraints of one allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocRequest {
    /// Bytes, rounded up to pages
    pub size: u64,
    /// A power of two; at least a page is used
    pub align: u64,
    pub mem_type: MemoryType,
    /// The allocation lies entirely inside this range (and below [`ALLOCATION_LIMIT`])
    pub window: PhysRange,
    /// Exactly this address, if given
    pub address: Option<u64>,
    pub strategy: super::MemoryAllocationStrategy,
}

impl AllocRequest {
    /// The whole range MiniOS allocates from
    pub const DEFAULT_WINDOW: PhysRange = PhysRange {
        start: PAGE_SIZE,
        end: ALLOCATION_LIMIT,
    };

    #[inline]
    pub const fn new(size: u64, align: u64, mem_type: MemoryType) -> Self {
        Self {
            size,
            align,
            mem_type,
            window: Self::DEFAULT_WINDOW,
            address: None,
            strategy: super::MemoryAllocationStrategy::FirstFit,
        }
    }
}

/// The map table: a sorted array of regions in memory supplied by its owner
pub struct RegionTable {
    ptr: NonNull<MemoryRegion>,
    cap: usize,
    len: usize,
    peak: usize,
    /// Where allocations may come from
    window: PhysRange,
}

impl RegionTable {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            ptr: NonNull::dangling(),
            cap: 0,
            len: 0,
            peak: 0,
            window: AllocRequest::DEFAULT_WINDOW,
        }
    }

    /// # Safety
    ///
    /// `ptr` must be valid for `cap` entries for as long as the table is used,
    /// and nothing else may use that memory.
    #[inline]
    pub unsafe fn from_raw(ptr: NonNull<MemoryRegion>, cap: usize) -> Self {
        Self {
            ptr,
            cap,
            len: 0,
            peak: 0,
            window: AllocRequest::DEFAULT_WINDOW,
        }
    }

    /// Host tests allocate from buffers wherever the host put them.
    #[cfg(test)]
    pub fn set_window(&mut self, window: PhysRange) {
        self.window = window;
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub const fn capacity(&self) -> usize {
        self.cap
    }

    /// The largest number of entries used so far
    #[inline]
    pub const fn peak(&self) -> usize {
        self.peak
    }

    #[inline]
    pub fn as_slice(&self) -> &[MemoryRegion] {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    #[inline]
    fn as_mut_slice(&mut self) -> &mut [MemoryRegion] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }

    /// Replaces the contents with a normalized list.
    pub fn load(&mut self, regions: &[MemoryRegion]) -> Result<(), MapError> {
        if regions.len() > self.cap {
            return Err(MapError::CapacityExceeded);
        }
        let sorted = regions
            .windows(2)
            .all(|w| w[0].range.end <= w[1].range.start);
        if !sorted {
            return Err(MapError::InvalidRange);
        }
        unsafe {
            core::ptr::copy_nonoverlapping(regions.as_ptr(), self.ptr.as_ptr(), regions.len());
        }
        self.len = regions.len();
        self.peak = self.peak.max(self.len);
        Ok(())
    }

    fn insert(&mut self, index: usize, region: MemoryRegion) {
        debug_assert!(self.len < self.cap && index <= self.len);
        unsafe {
            let p = self.ptr.as_ptr().add(index);
            core::ptr::copy(p, p.add(1), self.len - index);
            p.write(region);
        }
        self.len += 1;
        self.peak = self.peak.max(self.len);
    }

    fn remove(&mut self, index: usize) {
        debug_assert!(index < self.len);
        unsafe {
            let p = self.ptr.as_ptr().add(index);
            core::ptr::copy(p.add(1), p, self.len - index - 1);
        }
        self.len -= 1;
    }

    /// Index of the region that contains `address`
    pub fn find(&self, address: u64) -> Option<usize> {
        let slice = self.as_slice();
        let index = slice.partition_point(|v| v.range.end <= address);
        slice
            .get(index)
            .is_some_and(|v| v.range.contains(address))
            .then_some(index)
    }

    /// Splits the region containing `address` so that a region starts there.
    /// The caller has checked that there is room.
    fn split_at(&mut self, address: u64) {
        if let Some(index) = self.find(address) {
            let region = self.as_slice()[index];
            if region.range.start < address {
                self.as_mut_slice()[index].range.end = address;
                let mut tail = region;
                tail.range.start = address;
                self.insert(index + 1, tail);
            }
        }
    }

    fn needs_split(&self, address: u64) -> bool {
        self.find(address)
            .is_some_and(|i| self.as_slice()[i].range.start < address)
    }

    /// Merges the neighbours of `index` into it where nothing is lost.
    fn merge_around(&mut self, mut index: usize) {
        if index > 0 {
            let slice = self.as_slice();
            if slice[index - 1].can_merge(&slice[index]) {
                let end = slice[index].range.end;
                self.as_mut_slice()[index - 1].range.end = end;
                self.remove(index);
                index -= 1;
            }
        }
        if index + 1 < self.len {
            let slice = self.as_slice();
            if slice[index].can_merge(&slice[index + 1]) {
                let end = slice[index + 1].range.end;
                self.as_mut_slice()[index].range.end = end;
                self.remove(index + 1);
            }
        }
    }

    /// Carves an allocation out of a free region.
    pub fn allocate(&mut self, request: &AllocRequest) -> Result<PhysRange, MemoryError> {
        if !request.mem_type.is_allocatable()
            || request.size == 0
            || !request.align.is_power_of_two()
        {
            return Err(MemoryError::InvalidParameter);
        }
        let size = request
            .size
            .checked_add(PAGE_SIZE - 1)
            .ok_or(MemoryError::InvalidParameter)?
            & PAGE_MASK;
        let align = request.align.max(PAGE_SIZE);
        let window = request
            .window
            .intersection(&self.window)
            .ok_or(MemoryError::OutOfMemory)?;

        let found = match request.address {
            Some(address) => self.place_at(address, size, align, &window)?,
            None => self.place(size, align, &window, request.strategy),
        };
        let (index, range) = found.ok_or(MemoryError::OutOfMemory)?;

        let region = self.as_slice()[index];
        let extra =
            (region.range.start < range.start) as usize + (range.end < region.range.end) as usize;
        if self.len + extra > self.cap {
            return Err(MemoryError::TableFull);
        }
        self.split_at(range.start);
        self.split_at(range.end);
        let index = self.find(range.start).unwrap();
        let entry = &mut self.as_mut_slice()[index];
        debug_assert_eq!(entry.range, range);
        entry.mem_type = request.mem_type;
        entry.attrs |= RegionAttrs::ALLOCATION;
        Ok(range)
    }

    fn place_at(
        &self,
        address: u64,
        size: u64,
        align: u64,
        window: &PhysRange,
    ) -> Result<Option<(usize, PhysRange)>, MemoryError> {
        if address & (align - 1) != 0 {
            return Err(MemoryError::InvalidParameter);
        }
        let end = address
            .checked_add(size)
            .ok_or(MemoryError::InvalidParameter)?;
        let range = PhysRange {
            start: address,
            end,
        };
        if !window.contains_range(&range) {
            return Ok(None);
        }
        Ok(self.find(address).and_then(|index| {
            let region = &self.as_slice()[index];
            (region.is_free() && region.range.contains_range(&range)).then_some((index, range))
        }))
    }

    fn place(
        &self,
        size: u64,
        align: u64,
        window: &PhysRange,
        strategy: super::MemoryAllocationStrategy,
    ) -> Option<(usize, PhysRange)> {
        use super::MemoryAllocationStrategy::*;
        let fit_low = |region: &MemoryRegion| {
            let usable = region.range.intersection(window)?;
            let start = usable.start.checked_add(align - 1)? & !(align - 1);
            let end = start.checked_add(size)?;
            (end <= usable.end).then_some(PhysRange { start, end })
        };
        let fit_high = |region: &MemoryRegion| {
            let usable = region.range.intersection(window)?;
            let start = usable.end.checked_sub(size)? & !(align - 1);
            (start >= usable.start).then_some(PhysRange {
                start,
                end: start + size,
            })
        };
        let free = self
            .as_slice()
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_free());
        match strategy {
            FirstFit => free.filter_map(|(i, v)| fit_low(v).map(|r| (i, r))).next(),
            LastFit => free
                .rev()
                .filter_map(|(i, v)| fit_high(v).map(|r| (i, r)))
                .next(),
            BestFit => free
                .filter_map(|(i, v)| fit_low(v).map(|r| (i, r, v.range.len())))
                .min_by_key(|v| v.2)
                .map(|(i, r, _)| (i, r)),
        }
    }

    /// Frees an allocation; `range` must be exactly the allocation.
    ///
    /// With `expected`, the allocation must have that use.
    pub fn free(
        &mut self,
        range: PhysRange,
        expected: Option<MemoryType>,
    ) -> Result<(), MemoryFreeError> {
        let index = self.check_allocation(range, expected)?;
        let entry = &mut self.as_mut_slice()[index];
        entry.mem_type = MemoryType::Available;
        entry.attrs -= RegionAttrs::ALLOCATION;
        self.merge_around(index);
        Ok(())
    }

    /// Changes the use of an allocation, e.g. a buffer that becomes boot data.
    pub fn retype(
        &mut self,
        range: PhysRange,
        from: MemoryType,
        to: MemoryType,
    ) -> Result<(), MemoryFreeError> {
        if !to.is_allocatable() {
            return Err(MemoryFreeError::InvalidParameter);
        }
        let index = self.check_allocation(range, Some(from))?;
        self.as_mut_slice()[index].mem_type = to;
        Ok(())
    }

    fn check_allocation(
        &self,
        range: PhysRange,
        expected: Option<MemoryType>,
    ) -> Result<usize, MemoryFreeError> {
        let index = self
            .find(range.start)
            .ok_or(MemoryFreeError::InvalidPointer)?;
        let region = &self.as_slice()[index];
        if region.is_free() {
            return Err(if region.range.contains_range(&range) {
                MemoryFreeError::DoubleFree
            } else {
                MemoryFreeError::SizeMismatch
            });
        }
        if !region.is_allocation() {
            return Err(MemoryFreeError::NotAllocated);
        }
        if region.range != range {
            return Err(MemoryFreeError::SizeMismatch);
        }
        if expected.is_some_and(|v| v != region.mem_type) {
            return Err(MemoryFreeError::TypeMismatch);
        }
        Ok(index)
    }

    /// Protects a range after the table has been built.
    ///
    /// The pages the range touches get the use, combined with any protection
    /// they already have. Pages outside every region are added only with
    /// `add_gaps` (a firmware memory map that lists more than the RAM seeded
    /// first); otherwise they are left out of the map. Nothing changes on an
    /// error.
    pub fn reserve(
        &mut self,
        range: PhysRange,
        mem_type: MemoryType,
        attrs: RegionAttrs,
        add_gaps: bool,
    ) -> Result<(), MapError> {
        if attrs.contains(RegionAttrs::ALLOCATION) {
            return Err(MapError::InvalidRange);
        }
        let range = range.page_outer()?;
        let slice = self.as_slice();
        let first = slice.partition_point(|v| v.range.end <= range.start);
        let last = slice.partition_point(|v| v.range.start < range.end);
        let overlapped = &slice[first..last];

        for region in overlapped {
            if region.is_allocation() {
                return Err(MapError::Allocated(region.range));
            }
            if is_conflict((region.mem_type, region.attrs), (mem_type, attrs)) {
                return Err(MapError::Conflict {
                    first: (region.range, region.mem_type),
                    second: (range, mem_type),
                });
            }
        }

        let mut needed =
            self.needs_split(range.start) as usize + self.needs_split(range.end) as usize;
        if add_gaps {
            needed += gaps(overlapped, range).count();
        }
        if self.len + needed > self.cap {
            return Err(MapError::CapacityExceeded);
        }

        if add_gaps {
            let gap_attrs = if matches!(mem_type, MemoryType::Available | MemoryType::Loader) {
                attrs | RegionAttrs::RAM
            } else {
                attrs
            };
            // Gaps are computed again for each insertion, since indices move.
            while let Some(gap) = {
                let slice = self.as_slice();
                let first = slice.partition_point(|v| v.range.end <= range.start);
                let last = slice.partition_point(|v| v.range.start < range.end);
                gaps(&slice[first..last], range).next()
            } {
                let index = self
                    .as_slice()
                    .partition_point(|v| v.range.end <= gap.start);
                self.insert(index, MemoryRegion::new(gap, mem_type, gap_attrs));
            }
        }

        self.split_at(range.start);
        self.split_at(range.end);
        let first = self
            .as_slice()
            .partition_point(|v| v.range.end <= range.start);
        let last = self
            .as_slice()
            .partition_point(|v| v.range.start < range.end);
        for region in &mut self.as_mut_slice()[first..last] {
            let (t, a) = combine((region.mem_type, region.attrs), (mem_type, attrs));
            region.mem_type = t;
            region.attrs = a;
        }
        let mut index = last;
        while index > first {
            index -= 1;
            if index < self.len {
                self.merge_around(index);
            }
        }
        Ok(())
    }

    /// The map as handed over: neighbours with the same use and attributes are
    /// merged, allocation boundaries dropped, [`ALLOCATION_LIMIT`] kept.
    pub fn coalesced(&self) -> impl Iterator<Item = MemoryRegion> + '_ {
        let mut iter = self
            .as_slice()
            .iter()
            .map(|v| MemoryRegion::new(v.range, v.mem_type, v.attrs & RegionAttrs::HANDOFF));
        let mut pending = iter.next();
        core::iter::from_fn(move || {
            let mut current = pending.take()?;
            for next in iter.by_ref() {
                if current.can_merge(&next) {
                    current.range.end = next.range.end;
                } else {
                    pending = Some(next);
                    break;
                }
            }
            Some(current)
        })
    }

    /// Bytes of RAM in the map
    pub fn ram_bytes(&self, window: PhysRange) -> u64 {
        self.sum(window, |v| v.attrs.contains(RegionAttrs::RAM))
    }

    /// Free bytes the allocator can use
    pub fn free_bytes(&self) -> u64 {
        self.sum(self.window, MemoryRegion::is_free)
    }

    /// The largest free block the allocator can use
    pub fn max_free_block(&self) -> u64 {
        self.as_slice()
            .iter()
            .filter(|v| v.is_free())
            .filter_map(|v| v.range.intersection(&self.window))
            .map(|v| v.len())
            .max()
            .unwrap_or(0)
    }

    fn sum(&self, window: PhysRange, f: impl Fn(&MemoryRegion) -> bool) -> u64 {
        self.as_slice()
            .iter()
            .filter(|v| f(v))
            .filter_map(|v| v.range.intersection(&window))
            .map(|v| v.len())
            .sum()
    }
}

/// The parts of `range` not covered by the sorted `regions`, split at [`ALLOCATION_LIMIT`]
fn gaps(regions: &[MemoryRegion], range: PhysRange) -> impl Iterator<Item = PhysRange> + '_ {
    let mut cursor = range.start;
    let mut iter = regions.iter();
    let mut done = false;
    core::iter::from_fn(move || {
        while !done {
            let start = cursor;
            let end = match iter.next() {
                Some(region) => {
                    cursor = cursor.max(region.range.end);
                    region.range.start.min(range.end)
                }
                None => {
                    done = true;
                    range.end
                }
            };
            if let Some(gap) = PhysRange::new(start, end) {
                return Some(gap);
            }
        }
        None
    })
    .flat_map(split_at_limit)
}

fn split_at_limit(range: PhysRange) -> impl Iterator<Item = PhysRange> {
    let (low, high) = if range.start < ALLOCATION_LIMIT && ALLOCATION_LIMIT < range.end {
        (
            PhysRange::new(range.start, ALLOCATION_LIMIT),
            PhysRange::new(ALLOCATION_LIMIT, range.end),
        )
    } else {
        (Some(range), None)
    };
    low.into_iter().chain(high)
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::mem::MemoryAllocationStrategy;

    const GIB: u64 = 0x4000_0000;

    fn r(start: u64, end: u64) -> PhysRange {
        PhysRange::new(start, end).unwrap()
    }

    fn build(
        ram: &[(u64, u64)],
        protected: &[(u64, u64, MemoryType, RegionAttrs)],
    ) -> Vec<MemoryRegion> {
        let mut builder = MapBuilder::<16, 32>::new();
        for &(s, e) in ram {
            builder.add_ram(r(s, e)).unwrap();
        }
        for &(s, e, t, a) in protected {
            builder.add_protected(r(s, e), t, a).unwrap();
        }
        let mut out =
            vec![MemoryRegion::new(r(0, 1), MemoryType::Available, RegionAttrs::empty()); 128];
        let len = builder.build(&mut out).unwrap();
        out.truncate(len);
        check_invariants(&out);
        out
    }

    fn check_invariants(map: &[MemoryRegion]) {
        for region in map {
            assert_eq!(region.range.start() % PAGE_SIZE, 0, "{:?}", region);
            assert_eq!(region.range.end() % PAGE_SIZE, 0, "{:?}", region);
            assert!(
                region.range.end() <= ALLOCATION_LIMIT || region.range.start() >= ALLOCATION_LIMIT,
                "{:?} crosses 4 GiB",
                region
            );
        }
        for w in map.windows(2) {
            assert!(
                w[0].range.end() <= w[1].range.start(),
                "{:?} {:?}",
                w[0],
                w[1]
            );
        }
    }

    fn table(regions: &[MemoryRegion], cap: usize) -> RegionTable {
        let storage = Vec::leak(vec![
            MemoryRegion::new(
                r(0, 1),
                MemoryType::Available,
                RegionAttrs::empty()
            );
            cap
        ]);
        let mut table =
            unsafe { RegionTable::from_raw(NonNull::new(storage.as_mut_ptr()).unwrap(), cap) };
        table.load(regions).unwrap();
        table
    }

    fn simple(regions: &[MemoryRegion]) -> Vec<(u64, u64, MemoryType)> {
        regions
            .iter()
            .map(|v| (v.range.start(), v.range.end(), v.mem_type))
            .collect()
    }

    fn free_ram(start: u64, end: u64) -> MemoryRegion {
        MemoryRegion::new(r(start, end), MemoryType::Available, RegionAttrs::RAM)
    }

    use MemoryType::*;
    const NONE: RegionAttrs = RegionAttrs::empty();

    #[test]
    fn ranges_use_checked_arithmetic() {
        assert_eq!(PhysRange::from_base_size(0x1000, 0), Ok(None));
        assert_eq!(
            PhysRange::from_base_size(u64::MAX, 2),
            Err(MapError::Overflow)
        );
        assert_eq!(
            PhysRange::from_bounds(0x2000, 0x1000),
            Err(MapError::InvalidRange)
        );
        assert_eq!(PhysRange::from_bounds(0x2000, 0x2000), Ok(None));
        assert_eq!(r(0x1001, 0x2fff).page_inner(), None);
        assert_eq!(r(0x1001, 0x3fff).page_inner(), Some(r(0x2000, 0x3000)));
        assert_eq!(r(0x1001, 0x2fff).page_outer(), Ok(r(0x1000, 0x3000)));
        assert_eq!(r(0, u64::MAX).page_outer(), Err(MapError::Overflow));
        assert_eq!(r(u64::MAX - 5, u64::MAX).page_inner(), None);
    }

    #[test]
    fn several_banks_in_any_order_are_normalized_the_same() {
        let a = build(
            &[(0x8000_0000, 0x9000_0000), (0x4000_0000, 0x5000_0000)],
            &[],
        );
        let b = build(
            &[(0x4000_0000, 0x5000_0000), (0x8000_0000, 0x9000_0000)],
            &[],
        );
        assert_eq!(a, b);
        assert_eq!(
            simple(&a),
            [
                (0x4000_0000, 0x5000_0000, Available),
                (0x8000_0000, 0x9000_0000, Available)
            ]
        );
    }

    #[test]
    fn overlapping_and_adjacent_banks_merge() {
        let map = build(
            &[
                (0x4000_0000, 0x4800_0000),
                (0x4400_0000, 0x5000_0000),
                (0x5000_0000, 0x5100_0000),
            ],
            &[],
        );
        assert_eq!(simple(&map), [(0x4000_0000, 0x5100_0000, Available)]);
    }

    #[test]
    fn ram_is_rounded_inward_and_protection_outward() {
        let map = build(
            &[(0x4000_0800, 0x4001_0800)],
            &[(0x4000_2010, 0x4000_2020, Reserved, RegionAttrs::MEMRESERVE)],
        );
        assert_eq!(
            simple(&map),
            [
                (0x4000_1000, 0x4000_2000, Available),
                (0x4000_2000, 0x4000_3000, Reserved),
                (0x4000_3000, 0x4001_0000, Available),
            ]
        );
    }

    #[test]
    fn zero_sized_ram_leaves_nothing() {
        let map = build(&[(0x4000_0800, 0x4000_0900)], &[]);
        assert!(map.is_empty());
    }

    #[test]
    fn ram_is_split_at_4_gib_and_high_ram_stays_available() {
        let map = build(&[(3 * GIB, 6 * GIB)], &[]);
        assert_eq!(
            simple(&map),
            [(3 * GIB, 4 * GIB, Available), (4 * GIB, 6 * GIB, Available)]
        );
        let only_high = build(&[(4 * GIB, 5 * GIB)], &[]);
        assert_eq!(simple(&only_high), [(4 * GIB, 5 * GIB, Available)]);
        let just_below = build(&[(4 * GIB - 0x1000, 4 * GIB)], &[]);
        assert_eq!(
            simple(&just_below),
            [(4 * GIB - 0x1000, 4 * GIB, Available)]
        );
    }

    #[test]
    fn reservations_outside_ram_do_not_become_ram() {
        let map = build(
            &[(0x4000_0000, 0x4010_0000)],
            &[(0x3ff0_0000, 0x4000_2000, Reserved, RegionAttrs::MEMRESERVE)],
        );
        assert_eq!(
            simple(&map),
            [
                (0x4000_0000, 0x4000_2000, Reserved),
                (0x4000_2000, 0x4010_0000, Available)
            ]
        );
    }

    #[test]
    fn a_known_object_in_a_reservation_keeps_its_use_and_the_attributes() {
        let map = build(
            &[(0x4000_0000, 0x4100_0000)],
            &[
                (
                    0x4000_0000,
                    0x4040_0000,
                    Reserved,
                    RegionAttrs::RESERVED_MEMORY | RegionAttrs::NO_MAP,
                ),
                (0x4010_0000, 0x4010_8000, DeviceTree, NONE),
            ],
        );
        let dt = map.iter().find(|v| v.mem_type == DeviceTree).unwrap();
        assert_eq!(dt.range, r(0x4010_0000, 0x4010_8000));
        assert!(
            dt.attrs
                .contains(RegionAttrs::NO_MAP | RegionAttrs::RESERVED_MEMORY)
        );
        assert_eq!(
            simple(&map),
            [
                (0x4000_0000, 0x4010_0000, Reserved),
                (0x4010_0000, 0x4010_8000, DeviceTree),
                (0x4010_8000, 0x4040_0000, Reserved),
                (0x4040_0000, 0x4100_0000, Available),
            ]
        );
    }

    #[test]
    fn duplicate_reservations_do_not_weaken_each_other() {
        for order in [false, true] {
            let mut protected = vec![
                (0x4000_0000, 0x4000_4000, Reserved, RegionAttrs::MEMRESERVE),
                (
                    0x4000_2000,
                    0x4000_6000,
                    Reserved,
                    RegionAttrs::RESERVED_MEMORY | RegionAttrs::NO_MAP,
                ),
            ];
            if order {
                protected.reverse();
            }
            let map = build(&[(0x4000_0000, 0x4001_0000)], &protected);
            let covered: u64 = map
                .iter()
                .filter(|v| v.mem_type == Reserved)
                .map(|v| v.range.len())
                .sum();
            assert_eq!(covered, 0x6000);
            let both = map.iter().find(|v| v.range.contains(0x4000_3000)).unwrap();
            assert!(
                both.attrs
                    .contains(RegionAttrs::MEMRESERVE | RegionAttrs::NO_MAP)
            );
        }
    }

    #[test]
    fn a_page_shared_by_rounding_takes_the_stronger_protection() {
        // The image ends and the DTB starts in the same page
        let map = build(
            &[(0x4000_0000, 0x4010_0000)],
            &[
                (0x4008_0000, 0x4008_4800, Loader, NONE),
                (0x4008_4800, 0x4008_6000, DeviceTree, NONE),
            ],
        );
        assert_eq!(
            simple(&map),
            [
                (0x4000_0000, 0x4008_0000, Available),
                (0x4008_0000, 0x4008_4000, Loader),
                (0x4008_4000, 0x4008_6000, DeviceTree),
                (0x4008_6000, 0x4010_0000, Available),
            ]
        );
    }

    #[test]
    fn conflicting_overlaps_are_rejected() {
        let cases = [
            (
                (0x4008_0000, 0x4009_0000, Loader, NONE),
                (0x4008_f000, 0x400a_0000, DeviceTree, NONE),
            ),
            (
                (0x4008_0000, 0x4009_0000, Loader, NONE),
                (0x4000_0000, 0x4008_1000, Reserved, RegionAttrs::MEMRESERVE),
            ),
            (
                (0x4008_0000, 0x4009_0000, Initrd, NONE),
                (0x4008_f000, 0x400a_0000, DeviceTree, NONE),
            ),
            (
                (0x4008_0000, 0x4009_0000, Framebuffer, NONE),
                (0x4008_0000, 0x4009_0000, OtherFw, NONE),
            ),
            (
                (0x4008_0000, 0x4009_0000, Reserved, RegionAttrs::NO_MAP),
                (0x4008_0000, 0x4009_0000, Reserved, RegionAttrs::REUSABLE),
            ),
        ];
        for (a, b) in cases {
            let mut builder = MapBuilder::<4, 4>::new();
            builder.add_ram(r(0x4000_0000, 0x5000_0000)).unwrap();
            builder.add_protected(r(a.0, a.1), a.2, a.3).unwrap();
            builder.add_protected(r(b.0, b.1), b.2, b.3).unwrap();
            let mut out = [free_ram(0, 0x1000); 16];
            assert!(
                matches!(builder.build(&mut out), Err(MapError::Conflict { .. })),
                "{:?} {:?}",
                a,
                b
            );
        }
    }

    #[test]
    fn inputs_beyond_capacity_are_an_error() {
        let mut builder = MapBuilder::<1, 1>::new();
        builder.add_ram(r(0, 0x1000)).unwrap();
        assert_eq!(
            builder.add_ram(r(0x1000, 0x2000)),
            Err(MapError::CapacityExceeded)
        );
        builder.add_protected(r(0, 0x10), Reserved, NONE).unwrap();
        assert_eq!(
            builder.add_protected(r(0x10, 0x20), Reserved, NONE),
            Err(MapError::CapacityExceeded)
        );
        let mut builder = MapBuilder::<2, 2>::new();
        builder.add_ram(r(0x1000, 0x10000)).unwrap();
        builder
            .add_protected(r(0x3000, 0x4000), Reserved, NONE)
            .unwrap();
        let mut out = [free_ram(0, 0x1000); 2];
        assert_eq!(builder.build(&mut out), Err(MapError::CapacityExceeded));
    }

    #[test]
    fn allocations_never_touch_protected_or_high_memory() {
        let map = build(
            &[(0x4000_0000, 0x4001_0000), (3 * GIB, 5 * GIB)],
            &[
                (0x4000_0000, 0x4000_2000, Loader, NONE),
                (0x4000_4000, 0x4000_5000, DeviceTree, NONE),
                (0x4000_8000, 0x4000_9000, Initrd, NONE),
                (0x4000_c000, 0x4000_d000, Framebuffer, RegionAttrs::FIRMWARE),
            ],
        );
        let mut t = table(&map, 64);
        let protected: Vec<_> = map
            .iter()
            .filter(|v| !v.is_free())
            .map(|v| v.range)
            .collect();
        let mut got = Vec::new();
        for strategy in [
            MemoryAllocationStrategy::FirstFit,
            MemoryAllocationStrategy::LastFit,
        ] {
            for _ in 0..8 {
                let mut req = AllocRequest::new(PAGE_SIZE, PAGE_SIZE, Loader);
                req.strategy = strategy;
                let range = t.allocate(&req).unwrap();
                assert!(range.end() <= ALLOCATION_LIMIT);
                assert!(protected.iter().all(|p| !p.intersects(&range)));
                got.push(range);
            }
        }
        // Everything returned so far is distinct
        for (i, a) in got.iter().enumerate() {
            for b in &got[i + 1..] {
                assert!(!a.intersects(b));
            }
        }
        // High RAM is still free and in the map
        let high = t
            .as_slice()
            .iter()
            .find(|v| v.range.start() == 4 * GIB)
            .unwrap();
        assert_eq!(high.mem_type, Available);
        assert_eq!(high.range.end(), 5 * GIB);
        // A request that only high RAM could satisfy fails
        let req = AllocRequest::new(2 * GIB, PAGE_SIZE, Loader);
        assert_eq!(t.allocate(&req), Err(MemoryError::OutOfMemory));
    }

    #[test]
    fn allocations_from_several_banks() {
        let map = build(&[(0x1000, 0x3000), (0x10_0000, 0x10_2000)], &[]);
        let mut t = table(&map, 16);
        let sizes = [0x2000, 0x2000];
        let got: Vec<_> = sizes
            .iter()
            .map(|&s| {
                t.allocate(&AllocRequest::new(s, PAGE_SIZE, Loader))
                    .unwrap()
            })
            .collect();
        assert_eq!(got, [r(0x1000, 0x3000), r(0x10_0000, 0x10_2000)]);
        assert_eq!(t.free_bytes(), 0);
        assert_eq!(
            t.allocate(&AllocRequest::new(0x1000, PAGE_SIZE, Loader)),
            Err(MemoryError::OutOfMemory)
        );
    }

    #[test]
    fn page_zero_is_never_returned() {
        let map = build(&[(0, 0x4000)], &[]);
        let mut t = table(&map, 8);
        let range = t
            .allocate(&AllocRequest::new(0x1000, PAGE_SIZE, Loader))
            .unwrap();
        assert_eq!(range, r(0x1000, 0x2000));
    }

    #[test]
    fn address_alignment_and_window_constraints() {
        let map = build(&[(0x4000_0000, 0x5000_0000)], &[]);
        let mut t = table(&map, 32);

        let mut req = AllocRequest::new(0x3000, 0x1_0000, Loader);
        req.window = r(0x4800_1000, 0x4900_0000);
        let range = t.allocate(&req).unwrap();
        assert_eq!(range, r(0x4801_0000, 0x4801_3000));

        let mut req = AllocRequest::new(0x1000, PAGE_SIZE, Kernel);
        req.address = Some(0x4400_0000);
        assert_eq!(t.allocate(&req), Ok(r(0x4400_0000, 0x4400_1000)));
        // The same address again is taken
        assert_eq!(t.allocate(&req), Err(MemoryError::OutOfMemory));
        // Misaligned
        req.address = Some(0x4400_2800);
        assert_eq!(t.allocate(&req), Err(MemoryError::InvalidParameter));
        // Running past the end of the RAM
        req.address = Some(0x4fff_f000);
        req.size = 0x2000;
        assert_eq!(t.allocate(&req), Err(MemoryError::OutOfMemory));
        // A window nothing fits in
        let mut req = AllocRequest::new(0x2000, PAGE_SIZE, Loader);
        req.window = r(0x1000, 0x4000_1000);
        assert_eq!(t.allocate(&req), Err(MemoryError::OutOfMemory));
        // Available is not a use
        let req = AllocRequest::new(0x1000, PAGE_SIZE, Available);
        assert_eq!(t.allocate(&req), Err(MemoryError::InvalidParameter));
    }

    #[test]
    fn best_fit_picks_the_smallest_block() {
        let map = build(
            &[(0x1_0000, 0x8_0000)],
            &[
                (0x2_0000, 0x3_0000, Reserved, NONE),
                (0x3_2000, 0x4_0000, Reserved, NONE),
            ],
        );
        let mut t = table(&map, 16);
        let mut req = AllocRequest::new(0x2000, PAGE_SIZE, Loader);
        req.strategy = MemoryAllocationStrategy::BestFit;
        assert_eq!(t.allocate(&req), Ok(r(0x3_0000, 0x3_2000)));
    }

    #[test]
    fn a_full_table_leaves_everything_unchanged() {
        let map = build(&[(0x1_0000, 0x10_0000)], &[]);
        let mut t = table(&map, 2);
        // Splitting the only region in three needs two more entries
        let mut req = AllocRequest::new(0x1000, PAGE_SIZE, Loader);
        req.address = Some(0x2_0000);
        let before: Vec<_> = t.as_slice().to_vec();
        assert_eq!(t.allocate(&req), Err(MemoryError::TableFull));
        assert_eq!(t.as_slice(), &before[..]);
        // From the start of the region it needs one
        let first = t
            .allocate(&AllocRequest::new(0x1000, PAGE_SIZE, Loader))
            .unwrap();
        assert_eq!(first, r(0x1_0000, 0x1_1000));
        assert_eq!(t.len(), 2);
        let before: Vec<_> = t.as_slice().to_vec();
        assert_eq!(
            t.allocate(&AllocRequest::new(0x1000, PAGE_SIZE, Loader)),
            Err(MemoryError::TableFull)
        );
        assert_eq!(t.as_slice(), &before[..]);
        // Reservations are all or nothing as well
        assert_eq!(
            t.reserve(r(0x5_0000, 0x5_1000), Reserved, NONE, false),
            Err(MapError::CapacityExceeded)
        );
        assert_eq!(t.as_slice(), &before[..]);
    }

    #[test]
    fn neighbouring_allocations_are_freed_one_by_one() {
        let map = build(&[(0x1_0000, 0x2_0000)], &[]);
        let mut t = table(&map, 16);
        let a = t
            .allocate(&AllocRequest::new(0x1000, PAGE_SIZE, Loader))
            .unwrap();
        let b = t
            .allocate(&AllocRequest::new(0x1000, PAGE_SIZE, Loader))
            .unwrap();
        let c = t
            .allocate(&AllocRequest::new(0x1000, PAGE_SIZE, Loader))
            .unwrap();
        assert_eq!(b.start(), a.end());
        assert_eq!(t.len(), 4);
        t.free(b, Some(Loader)).unwrap();
        assert_eq!(t.as_slice()[1].range, b);
        assert!(t.as_slice()[1].is_free());
        t.free(a, None).unwrap();
        t.free(c, Some(Loader)).unwrap();
        // All merged back into one free region
        assert_eq!(simple(t.as_slice()), [(0x1_0000, 0x2_0000, Available)]);
        assert_eq!(t.as_slice(), &map[..]);
    }

    #[test]
    fn invalid_frees_are_detected_without_change() {
        let map = build(
            &[(0x1_0000, 0x2_0000)],
            &[(0x1_8000, 0x1_9000, Framebuffer, RegionAttrs::FIRMWARE)],
        );
        let mut t = table(&map, 16);
        let a = t
            .allocate(&AllocRequest::new(0x2000, PAGE_SIZE, Loader))
            .unwrap();
        let before: Vec<_> = t.as_slice().to_vec();
        assert_eq!(
            t.free(r(a.start(), a.start() + 0x1000), None),
            Err(MemoryFreeError::SizeMismatch)
        );
        assert_eq!(
            t.free(r(a.start() + 0x1000, a.end()), None),
            Err(MemoryFreeError::SizeMismatch)
        );
        assert_eq!(t.free(a, Some(Kernel)), Err(MemoryFreeError::TypeMismatch));
        assert_eq!(
            t.free(r(0x1_8000, 0x1_9000), None),
            Err(MemoryFreeError::NotAllocated)
        );
        assert_eq!(
            t.free(r(0x9_0000, 0x9_1000), None),
            Err(MemoryFreeError::InvalidPointer)
        );
        assert_eq!(t.as_slice(), &before[..]);
        t.free(a, Some(Loader)).unwrap();
        assert_eq!(t.free(a, Some(Loader)), Err(MemoryFreeError::DoubleFree));
    }

    #[test]
    fn retype_changes_only_a_whole_allocation() {
        let map = build(&[(0x1_0000, 0x2_0000)], &[]);
        let mut t = table(&map, 16);
        let a = t
            .allocate(&AllocRequest::new(0x2000, PAGE_SIZE, Loader))
            .unwrap();
        assert_eq!(
            t.retype(a, Kernel, BootData),
            Err(MemoryFreeError::TypeMismatch)
        );
        assert_eq!(
            t.retype(a, Loader, Reserved),
            Err(MemoryFreeError::InvalidParameter)
        );
        t.retype(a, Loader, BootData).unwrap();
        assert_eq!(t.as_slice()[0].mem_type, BootData);
        assert_eq!(t.free(a, Some(Loader)), Err(MemoryFreeError::TypeMismatch));
    }

    #[test]
    fn late_reservations_protect_ram_and_skip_non_ram() {
        let map = build(&[(0x1_0000, 0x2_0000)], &[]);
        let mut t = table(&map, 16);
        let a = t
            .allocate(&AllocRequest::new(0x1000, PAGE_SIZE, Loader))
            .unwrap();
        // Over an allocation: refused
        assert_eq!(
            t.reserve(a, Framebuffer, RegionAttrs::FIRMWARE, false),
            Err(MapError::Allocated(a))
        );
        // Straddling the end of the RAM: only the RAM part is added
        t.reserve(
            r(0x1_f800, 0x3_0000),
            Framebuffer,
            RegionAttrs::FIRMWARE,
            false,
        )
        .unwrap();
        let last = t.as_slice().last().unwrap();
        assert_eq!(
            (last.range, last.mem_type),
            (r(0x1_f000, 0x2_0000), Framebuffer)
        );
        // Entirely outside: no change
        let before: Vec<_> = t.as_slice().to_vec();
        t.reserve(r(0x8000_0000, 0x8100_0000), Framebuffer, NONE, false)
            .unwrap();
        assert_eq!(t.as_slice(), &before[..]);
        // Firmware maps (x86) add what is outside, split at 4 GiB
        t.reserve(r(3 * GIB, 5 * GIB), Reserved, RegionAttrs::FIRMWARE, true)
            .unwrap();
        let tail: Vec<_> = simple(t.as_slice()).into_iter().rev().take(2).collect();
        assert_eq!(
            tail,
            [(4 * GIB, 5 * GIB, Reserved), (3 * GIB, 4 * GIB, Reserved)]
        );
        check_invariants(t.as_slice());
    }

    #[test]
    fn firmware_ram_added_later_is_allocatable() {
        let map = build(&[(0x10_0000, 0x20_0000)], &[]);
        let mut t = table(&map, 16);
        t.reserve(
            r(0x100_0000, 0x200_0000),
            Available,
            RegionAttrs::empty(),
            true,
        )
        .unwrap();
        let region = t.as_slice().last().unwrap();
        assert!(region.is_free() && region.attrs.contains(RegionAttrs::RAM));
        let mut req = AllocRequest::new(0x1000, PAGE_SIZE, Loader);
        req.address = Some(0x100_0000);
        assert!(t.allocate(&req).is_ok());
        // Available does not lift an existing protection
        t.reserve(r(0x10_0000, 0x10_1000), Reserved, NONE, true)
            .unwrap();
        t.reserve(r(0x10_0000, 0x10_1000), Available, NONE, true)
            .unwrap();
        assert_eq!(t.as_slice()[0].mem_type, Reserved);
    }

    #[test]
    fn the_handoff_map_merges_neighbours_but_keeps_uses_and_4_gib() {
        let map = build(&[(0x1_0000, 0x2_0000), (3 * GIB, 5 * GIB)], &[]);
        let mut t = table(&map, 32);
        for _ in 0..3 {
            t.allocate(&AllocRequest::new(0x1000, PAGE_SIZE, Loader))
                .unwrap();
        }
        let k = t
            .allocate(&AllocRequest::new(0x1000, PAGE_SIZE, Kernel))
            .unwrap();
        let out: Vec<_> = t.coalesced().collect();
        assert_eq!(
            simple(&out),
            [
                (0x1_0000, 0x1_3000, Loader),
                (0x1_3000, 0x1_4000, Kernel),
                (0x1_4000, 0x2_0000, Available),
                (3 * GIB, 4 * GIB, Available),
                (4 * GIB, 5 * GIB, Available),
            ]
        );
        assert!(out.iter().all(|v| !v.is_allocation()));
        // The table itself still has each allocation
        assert_eq!(t.len(), 7);
        assert_eq!(t.find(k.start()).map(|i| t.as_slice()[i].range), Some(k));
    }

    #[test]
    fn totals_count_ram_once() {
        let map = build(
            &[
                (0x1_0000, 0x2_0000),
                (0x1_8000, 0x3_0000),
                (3 * GIB, 5 * GIB),
            ],
            &[
                (0x1_0000, 0x1_4000, Reserved, NONE),
                (0x1_2000, 0x1_6000, Reserved, NONE),
                // A framebuffer outside the RAM is not counted
                (0x8000_0000, 0x8100_0000, Framebuffer, NONE),
            ],
        );
        let t = table(&map, 16);
        assert_eq!(t.ram_bytes(r(0, ALLOCATION_LIMIT)), 0x2_0000 + GIB);
        assert_eq!(t.ram_bytes(r(ALLOCATION_LIMIT, u64::MAX)), GIB);
        assert_eq!(t.free_bytes(), 0x2_0000 - 0x6000 + GIB);
        assert_eq!(t.max_free_block(), GIB);
    }
}

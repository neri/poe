//! Memory Manager
//!
//! Keeps one map of physical memory: every region of RAM with its use
//! ([`MemoryType`]) and its origin ([`RegionAttrs`]), sorted and without
//! overlaps. The allocator hands out whole pages from `Available` regions
//! below [`ALLOCATION_LIMIT`]; each allocation keeps its own entry until it is
//! freed. The table lives in RAM taken from the map itself, with a fixed
//! capacity ([`MemoryManager::TABLE_CAPACITY`]); it never grows.
//!
//! Before an OS takes over, [`MemoryManager::finalize_map`] writes the map
//! for it into `BootData` memory and freezes the table: from then on nothing
//! is allocated, freed or retyped.

use core::alloc::Layout;
use core::cell::UnsafeCell;
use core::ops::Range;
use core::ptr::NonNull;

use bootprot::{BootMemoryAttributes, BootMemoryRegion, BootMemoryType};

use super::region::{MapBuilder, PAGE_SIZE, ProtectedInput, RegionTable};
use super::{
    ALLOCATION_LIMIT, AllocRequest, MapError, MemoryAllocationStrategy, MemoryError,
    MemoryFreeError, MemoryMapEntry, MemoryRegion, MemoryType, PhysRange, RegionAttrs,
};
#[cfg(target_arch = "x86")]
use crate::arch::lomem::LoMemoryManager;
use crate::*;

static mut MM: UnsafeCell<MemoryManager> = UnsafeCell::new(MemoryManager::new());

#[cfg(feature = "device_tree")]
/// Capacity for RAM ranges collected at boot
pub const EARLY_RAM_CAPACITY: usize = 32;
#[cfg(feature = "device_tree")]
/// Capacity for protected ranges collected at boot
pub const EARLY_PROTECTED_CAPACITY: usize = 128;
#[cfg(feature = "device_tree")]
/// Capacity of the normalized map built at boot
const EARLY_MAP_CAPACITY: usize = 2 * (EARLY_RAM_CAPACITY + EARLY_PROTECTED_CAPACITY) + 1;

/// Collector of the boot inputs; static so that the early stack stays small.
/// It is part of the MiniOS image, which is protected.
#[cfg(feature = "device_tree")]
pub type EarlyMapBuilder = MapBuilder<EARLY_RAM_CAPACITY, EARLY_PROTECTED_CAPACITY>;
#[cfg(feature = "device_tree")]
static mut EARLY_BUILDER: EarlyMapBuilder = MapBuilder::new();
#[cfg(feature = "device_tree")]
static mut EARLY_MAP: [MemoryRegion; EARLY_MAP_CAPACITY] = [MemoryRegion::new(
    match PhysRange::new(0, PAGE_SIZE) {
        Some(v) => v,
        None => unreachable!(),
    },
    MemoryType::Available,
    RegionAttrs::empty(),
); EARLY_MAP_CAPACITY];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Uninitialized,
    Ready,
    /// The final map has been produced
    Frozen,
}

pub struct MemoryManager {
    map: MapState,
    allocation_strategy: MemoryAllocationStrategy,
}

/// The table and whether it may still change
struct MapState {
    table: RegionTable,
    state: State,
}

impl MapState {
    const fn new() -> Self {
        Self {
            table: RegionTable::empty(),
            state: State::Uninitialized,
        }
    }

    fn ready(&self) -> Result<(), MemoryError> {
        match self.state {
            State::Uninitialized => Err(MemoryError::NotInitialized),
            State::Frozen => Err(MemoryError::Frozen),
            State::Ready => Ok(()),
        }
    }

    fn ready_for_free(&self) -> Result<(), MemoryFreeError> {
        match self.state {
            State::Uninitialized => Err(MemoryFreeError::InvalidPointer),
            State::Frozen => Err(MemoryFreeError::Frozen),
            State::Ready => Ok(()),
        }
    }

    fn allocate(&mut self, request: &AllocRequest) -> Result<PhysRange, MemoryError> {
        self.ready()?;
        self.table.allocate(request)
    }

    fn free(&mut self, range: PhysRange, mem_type: MemoryType) -> Result<(), MemoryFreeError> {
        self.ready_for_free()?;
        self.table.free(range, Some(mem_type))
    }

    fn retype(
        &mut self,
        range: PhysRange,
        from: MemoryType,
        to: MemoryType,
    ) -> Result<(), MemoryFreeError> {
        self.ready_for_free()?;
        self.table.retype(range, from, to)
    }

    fn update(
        &mut self,
        f: impl FnOnce(&mut RegionTable) -> Result<(), MapError>,
    ) -> Result<(), MemoryError> {
        self.ready()?;
        f(&mut self.table).map_err(|err| match err {
            MapError::CapacityExceeded => MemoryError::TableFull,
            _ => MemoryError::InvalidParameter,
        })
    }

    /// Writes the final map and freezes; on an error nothing changes.
    ///
    /// # Safety
    ///
    /// The free regions of the table must be memory this may write to.
    unsafe fn finalize(&mut self) -> Result<&'static [BootMemoryRegion], MemoryError> {
        self.ready()?;
        let map = unsafe { finalize(&mut self.table) }?;
        self.state = State::Frozen;
        Ok(map)
    }

    fn freeze(&mut self) {
        if self.state == State::Ready {
            self.state = State::Frozen;
        }
    }
}

impl MemoryManager {
    pub const PAGE_SIZE: u64 = PAGE_SIZE;

    pub const PAGE_SIZE_M1: u64 = Self::PAGE_SIZE - 1;

    pub const PAGE_MASK: u64 = !(Self::PAGE_SIZE - 1);

    /// Entries of the map table. Every live allocation is one entry, and each
    /// run of free pages between them another. The QEMU and Raspberry Pi
    /// builds peak at a few hundred (see [`Self::table_usage`]).
    pub const TABLE_CAPACITY: usize = 2048;

    const fn new() -> Self {
        Self {
            map: MapState::new(),
            allocation_strategy: MemoryAllocationStrategy::FirstFit,
        }
    }

    #[inline]
    unsafe fn shared_mut<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut MM)).get_mut() }
    }

    #[inline]
    fn shared<'a>() -> &'a Self {
        unsafe { &*(&*(&raw const MM)).get() }
    }

    /// Initializes with the single conventional memory range from the SSBL.
    /// The platform registers the rest of its memory map afterwards.
    pub unsafe fn init() {
        let info = System::boot_info();
        let range = PhysRange::from_base_size(
            info.start_conventional_memory as u64,
            info.conventional_memory_size as u64,
        );
        let result = range
            .and_then(|v| v.and_then(|v| v.page_inner()).ok_or(MapError::InvalidRange))
            .and_then(|range| {
                let mut builder = MapBuilder::<1, 0>::new();
                builder.add_ram(range)?;
                let mut map =
                    [MemoryRegion::new(range, MemoryType::Available, RegionAttrs::RAM); 2];
                let len = builder.build(&mut map)?;
                unsafe { Self::install(&map[..len]) }
            });
        if let Err(err) = result {
            panic!("memory map: {}", err);
        }
    }

    /// Initializes from the device tree.
    ///
    /// All enabled memory nodes are RAM. The memory reservation block, the
    /// fixed `reg` of `/reserved-memory`, the DTB, an initrd from `/chosen`
    /// and what `in_use` adds (the MiniOS image, firmware framebuffers, ...)
    /// are protected. The DTB is only read. A malformed or conflicting input
    /// stops the system: memory that may be in use is never handed out.
    #[cfg(feature = "device_tree")]
    pub unsafe fn init_dt(
        dt: &fdt::DeviceTree,
        in_use: impl FnOnce(&mut EarlyMapBuilder) -> Result<(), MapError>,
    ) {
        let builder = unsafe { &mut *(&raw mut EARLY_BUILDER) };
        let map = unsafe { &mut *(&raw mut EARLY_MAP) };
        builder.clear();
        let result = super::dt::collect(dt, builder).and_then(|summary| {
            in_use(builder)?;
            let len = builder.build(map)?;
            Ok((summary, len))
        });
        let (summary, len) = match result {
            Ok(v) => v,
            Err(err) => {
                Self::print_inputs(builder);
                panic!("memory map from the device tree: {}", err);
            }
        };

        Self::print_inputs(builder);
        if summary.dynamic_reservations > 0 {
            println!(
                "DT reserved-memory: {} dynamic request(s) left to the OS",
                summary.dynamic_reservations
            );
        }
        if let Err(err) = unsafe { Self::install(&map[..len]) } {
            panic!("memory map: {}", err);
        }
        Self::print_map();
    }

    #[cfg(feature = "device_tree")]
    fn print_inputs(builder: &EarlyMapBuilder) {
        for ram in builder.ram() {
            println!("DT RAM:      {}", ram);
        }
        for item in builder.protected() {
            let region = MemoryRegion::new(item.range, item.mem_type, item.attrs);
            println!("DT PROTECT:  {}", region);
        }
    }

    #[cfg(feature = "device_tree")]
    fn print_map() {
        let shared = Self::shared();
        for region in shared.map.table.as_slice() {
            println!("MEMMAP:      {}", region);
        }
        println!(
            "Memory: {} KB RAM, {} KB free below 4 GB, table {}/{}",
            (shared
                .map
                .table
                .ram_bytes(PhysRange::new(0, u64::MAX).unwrap())
                + 1023)
                >> 10,
            (shared.map.table.free_bytes() + 1023) >> 10,
            shared.map.table.len(),
            shared.map.table.capacity(),
        );
    }

    /// Places the table in the highest free RAM below the allocation limit
    /// that fits, then loads `regions` into it with the table as `Loader`.
    unsafe fn install(regions: &[MemoryRegion]) -> Result<(), MapError> {
        let table_bytes = (Self::TABLE_CAPACITY * core::mem::size_of::<MemoryRegion>()) as u64;
        let table_bytes = (table_bytes + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let window = AllocRequest::DEFAULT_WINDOW;
        let place = regions
            .iter()
            .rev()
            .filter(|v| v.is_free())
            .filter_map(|v| v.range.intersection(&window))
            .find_map(|v| {
                PhysRange::new(v.end() - table_bytes, v.end()).filter(|_| v.len() >= table_bytes)
            })
            .ok_or(MapError::CapacityExceeded)?;

        let map = unsafe { &mut Self::shared_mut().map };
        let ptr = NonNull::new(place.start() as usize as *mut MemoryRegion)
            .ok_or(MapError::InvalidRange)?;
        map.table = unsafe { RegionTable::from_raw(ptr, Self::TABLE_CAPACITY) };
        map.table.load(regions)?;
        map.table
            .reserve(place, MemoryType::Loader, RegionAttrs::empty(), false)?;
        map.state = State::Ready;
        Ok(())
    }

    /// Adds a range from a firmware memory map (x86).
    ///
    /// Ranges outside the map are added as they are; within the map, the
    /// range takes the stronger protection. Allocated memory cannot be
    /// registered.
    pub unsafe fn register_memmap(
        range: Range<u64>,
        mem_type: MemoryType,
    ) -> Result<(), MemoryError> {
        let range = PhysRange::new(range.start, range.end).ok_or(MemoryError::InvalidParameter)?;
        let attrs = if mem_type == MemoryType::Loader || mem_type == MemoryType::Available {
            RegionAttrs::RAM
        } else {
            RegionAttrs::FIRMWARE
        };
        unsafe { Self::update(|table| table.reserve(range, mem_type, attrs, true)) }
    }

    /// Protects a range found after the map was built, such as a framebuffer
    /// set up by the firmware. Parts outside the RAM are not added to the map.
    pub unsafe fn reserve(
        range: PhysRange,
        mem_type: MemoryType,
        attrs: RegionAttrs,
    ) -> Result<(), MemoryError> {
        unsafe { Self::update(|table| table.reserve(range, mem_type, attrs, false)) }
    }

    unsafe fn update(
        f: impl FnOnce(&mut RegionTable) -> Result<(), MapError>,
    ) -> Result<(), MemoryError> {
        let map = unsafe { &mut Self::shared_mut().map };
        unsafe { without_interrupts!(map.update(f)) }
    }

    /// The regions of the map
    pub fn regions<'a>() -> impl Iterator<Item = MemoryRegion> + 'a {
        Self::shared().map.table.as_slice().iter().copied()
    }

    #[cfg(target_arch = "x86")]
    #[inline]
    pub fn memory_list<'a>() -> impl Iterator<Item = MemoryMapEntry> + 'a {
        LoMemoryManager::memory_list().chain(Self::regions().map(MemoryMapEntry::from))
    }

    #[cfg(not(target_arch = "x86"))]
    #[inline]
    pub fn memory_list<'a>() -> impl Iterator<Item = MemoryMapEntry> + 'a {
        Self::regions().map(MemoryMapEntry::from)
    }

    /// Bytes of RAM below 4 GB (on x86, the first megabyte included)
    #[inline]
    pub fn total_memory_size() -> usize {
        let shared = Self::shared();
        let low = shared
            .map
            .table
            .ram_bytes(PhysRange::new(0, ALLOCATION_LIMIT).unwrap());
        let low = if cfg!(target_arch = "x86") {
            low + 0x10_0000
        } else {
            low
        };
        low.min(usize::MAX as u64) as usize
    }

    /// Megabytes of RAM at and above 4 GB, which MiniOS does not allocate from
    #[inline]
    pub fn total_extended_memory_size() -> usize {
        let shared = Self::shared();
        let high = shared
            .map
            .table
            .ram_bytes(PhysRange::new(ALLOCATION_LIMIT, u64::MAX).unwrap());
        ((high + 0xfffff) >> 20) as usize
    }

    /// Free bytes the allocator can use
    pub fn free_memory_count() -> usize {
        Self::shared().map.table.free_bytes() as usize
    }

    /// The largest block the allocator can return
    pub fn max_free_memory_size() -> usize {
        Self::shared().map.table.max_free_block() as usize
    }

    /// (entries in use, largest number used so far, capacity) of the map table
    pub fn table_usage() -> (usize, usize, usize) {
        let table = &Self::shared().map.table;
        (table.len(), table.peak(), table.capacity())
    }

    #[inline]
    pub fn set_allocation_strategy(strategy: MemoryAllocationStrategy) {
        let shared = unsafe { Self::shared_mut() };
        shared.allocation_strategy = strategy;
    }

    #[inline]
    pub fn allocation_strategy() -> MemoryAllocationStrategy {
        let shared = Self::shared();
        shared.allocation_strategy
    }

    /// Allocates zeroed pages.
    #[must_use]
    pub fn zalloc(
        layout: Layout,
        desired_addr: Option<NonNullPhysicalAddress>,
        mem_type: MemoryType,
        strategy: Option<MemoryAllocationStrategy>,
    ) -> Result<*mut u8, MemoryError> {
        let mut request = AllocRequest::new(layout.size() as u64, layout.align() as u64, mem_type);
        request.address = desired_addr.map(|v| v.get().as_u64());
        request.strategy = strategy.unwrap_or(Self::allocation_strategy());
        Self::alloc_pages(&request).map(|v| v.as_ptr())
    }

    /// Allocates zeroed pages under the constraints of `request`.
    pub fn alloc_pages(request: &AllocRequest) -> Result<NonNull<u8>, MemoryError> {
        if request.size > i32::MAX as u64 || request.align > i32::MAX as u64 {
            return Err(MemoryError::InvalidParameter);
        }
        let map = unsafe { &mut Self::shared_mut().map };
        let range = unsafe { without_interrupts!(map.allocate(request)) }?;
        let ptr = range.start() as usize as *mut u8;
        unsafe {
            ptr.write_bytes(0, range.len() as usize);
        }
        NonNull::new(ptr).ok_or(MemoryError::OutOfMemory)
    }

    /// Allocates zeroed pages a device can reach at the same address as the
    /// CPU, entirely inside `window`.
    pub fn alloc_dma(len: usize, align: usize, window: PhysRange) -> Option<NonNull<u8>> {
        let mut request = AllocRequest::new(len as u64, align as u64, MemoryType::Loader);
        request.window = window;
        Self::alloc_pages(&request).ok()
    }

    /// Frees memory from [`Self::alloc_dma`].
    pub unsafe fn free_dma(ptr: NonNull<u8>, len: usize) -> Result<(), MemoryFreeError> {
        unsafe { Self::free_pages(ptr.as_ptr(), len, MemoryType::Loader) }
    }

    /// Frees a `Loader` allocation.
    pub unsafe fn zfree(ptr: *mut u8, layout: Layout) -> Result<(), MemoryFreeError> {
        if ptr.is_null() {
            // do nothing
            return Ok(());
        }
        unsafe { Self::free_pages(ptr, layout.size(), MemoryType::Loader) }
    }

    /// Frees an allocation of `mem_type`; `size` is the size it was allocated with.
    pub unsafe fn free_pages(
        ptr: *mut u8,
        size: usize,
        mem_type: MemoryType,
    ) -> Result<(), MemoryFreeError> {
        let range = Self::page_range(ptr, size)?;
        let map = unsafe { &mut Self::shared_mut().map };
        unsafe { without_interrupts!(map.free(range, mem_type)) }
    }

    /// Changes the use of an allocation, e.g. a buffer that is handed over.
    pub unsafe fn retype(
        ptr: *mut u8,
        size: usize,
        from: MemoryType,
        to: MemoryType,
    ) -> Result<(), MemoryFreeError> {
        let range = Self::page_range(ptr, size)?;
        let map = unsafe { &mut Self::shared_mut().map };
        unsafe { without_interrupts!(map.retype(range, from, to)) }
    }

    fn page_range(ptr: *mut u8, size: usize) -> Result<PhysRange, MemoryFreeError> {
        let start = ptr as usize as u64;
        let size = (size as u64)
            .max(1)
            .checked_add(PAGE_SIZE - 1)
            .ok_or(MemoryFreeError::InvalidParameter)?
            & !(PAGE_SIZE - 1);
        PhysRange::from_base_size(start, size)
            .ok()
            .flatten()
            .ok_or(MemoryFreeError::InvalidParameter)
    }

    /// Whether the final map has been produced
    pub fn is_frozen() -> bool {
        Self::shared().map.state == State::Frozen
    }

    /// Writes the final map for the next OS into a new `BootData` allocation
    /// and freezes the table.
    ///
    /// Call it after everything handed over has been allocated with its use.
    /// On an error the table is unchanged and not frozen. On success, the
    /// returned slice is the map; it is itself listed as `BootData`.
    pub unsafe fn finalize_map() -> Result<&'static [BootMemoryRegion], MemoryError> {
        let map = unsafe { &mut Self::shared_mut().map };
        unsafe { without_interrupts!(map.finalize()) }
    }

    /// Freezes the table without producing a map.
    pub fn freeze() {
        let map = unsafe { &mut Self::shared_mut().map };
        unsafe { without_interrupts!(map.freeze()) }
    }
}

/// Writes the coalesced map into a `BootData` allocation of its own.
unsafe fn finalize(table: &mut RegionTable) -> Result<&'static [BootMemoryRegion], MemoryError> {
    unsafe { finalize_in(table, AllocRequest::DEFAULT_WINDOW) }
}

unsafe fn finalize_in(
    table: &mut RegionTable,
    window: PhysRange,
) -> Result<&'static [BootMemoryRegion], MemoryError> {
    // The allocation splits one free region into at most three
    let capacity = table.coalesced().count() + 2;
    let bytes = capacity * core::mem::size_of::<BootMemoryRegion>();
    let mut request = AllocRequest::new(
        bytes as u64,
        core::mem::align_of::<BootMemoryRegion>() as u64,
        MemoryType::BootData,
    );
    request.window = window;
    let range = table.allocate(&request)?;
    let fail = |table: &mut RegionTable, err| {
        let _ = table.free(range, Some(MemoryType::BootData));
        Err(err)
    };
    let count = table.coalesced().count();
    if count > capacity {
        return fail(table, MemoryError::TableFull);
    }
    let covered = table
        .find(range.start())
        .map(|i| table.as_slice()[i])
        .is_some_and(|v| v.mem_type == MemoryType::BootData && v.range.contains_range(&range));
    if !covered {
        return fail(table, MemoryError::InvalidParameter);
    }
    let out = range.start() as usize as *mut BootMemoryRegion;
    unsafe {
        (out as *mut u8).write_bytes(0, range.len() as usize);
        for (index, region) in table.coalesced().enumerate() {
            out.add(index).write(to_boot_region(&region));
        }
        Ok(core::slice::from_raw_parts(out, count))
    }
}

/// Converts an internal region to the boot protocol's representation.
pub fn to_boot_region(region: &MemoryRegion) -> BootMemoryRegion {
    let mem_type = match region.mem_type {
        MemoryType::Loader => BootMemoryType::OsLoaderData,
        MemoryType::Available => BootMemoryType::Available,
        MemoryType::Reserved => BootMemoryType::Reserved,
        MemoryType::AcpiReclaim => BootMemoryType::AcpiReclaim,
        MemoryType::AcpiNvs => BootMemoryType::AcpiNonVolatile,
        MemoryType::DeviceTree => BootMemoryType::DeviceTree,
        MemoryType::OtherFw => BootMemoryType::FirmwareData,
        MemoryType::Kernel => BootMemoryType::Kernel,
        MemoryType::BootData => BootMemoryType::BootData,
        MemoryType::Initrd => BootMemoryType::Initrd,
        MemoryType::Framebuffer => BootMemoryType::Framebuffer,
    };
    let mut attributes = BootMemoryAttributes::empty();
    for (from, to) in [
        (RegionAttrs::RAM, BootMemoryAttributes::RAM),
        (RegionAttrs::NO_MAP, BootMemoryAttributes::NO_MAP),
        (RegionAttrs::REUSABLE, BootMemoryAttributes::REUSABLE),
        (RegionAttrs::MEMRESERVE, BootMemoryAttributes::MEMRESERVE),
        (
            RegionAttrs::RESERVED_MEMORY,
            BootMemoryAttributes::RESERVED_MEMORY,
        ),
        (RegionAttrs::FIRMWARE, BootMemoryAttributes::FIRMWARE),
    ] {
        if region.attrs.contains(from) {
            attributes = attributes.union(to);
        }
    }
    BootMemoryRegion {
        base: region.range.start(),
        size: region.range.len(),
        mem_type,
        attributes,
    }
}

impl From<MemoryRegion> for MemoryMapEntry {
    fn from(v: MemoryRegion) -> Self {
        MemoryMapEntry::new(v.range.start(), v.range.len(), v.mem_type)
    }
}

impl ProtectedInput {
    #[inline]
    pub const fn new(range: PhysRange, mem_type: MemoryType, attrs: RegionAttrs) -> Self {
        Self {
            range,
            mem_type,
            attrs,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;

    /// A table whose free RAM is a real buffer, so that the final map can be
    /// written, plus one high bank that is only listed.
    fn state_with_ram(pages: usize) -> (MapState, PhysRange) {
        let buffer = Vec::leak(vec![0u8; (pages + 1) * PAGE_SIZE as usize]);
        let start = (buffer.as_ptr() as u64 + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let ram = PhysRange::from_base_size(start, pages as u64 * PAGE_SIZE)
            .unwrap()
            .unwrap();
        let high = PhysRange::new(1 << 56, (1 << 56) + (1 << 30)).unwrap();
        let storage = Vec::leak(vec![
            MemoryRegion::new(
                ram,
                MemoryType::Available,
                RegionAttrs::empty()
            );
            64
        ]);
        let mut map = MapState::new();
        map.table =
            unsafe { RegionTable::from_raw(NonNull::new(storage.as_mut_ptr()).unwrap(), 64) };
        let mut regions = vec![MemoryRegion::new(
            ram,
            MemoryType::Available,
            RegionAttrs::RAM,
        )];
        if ram.end() <= high.start() {
            regions.push(MemoryRegion::new(
                high,
                MemoryType::Available,
                RegionAttrs::RAM,
            ));
        }
        map.table.load(&regions).unwrap();
        map.table.set_window(ram);
        map.state = State::Ready;
        (map, ram)
    }

    fn request(pages: u64, mem_type: MemoryType, ram: PhysRange) -> AllocRequest {
        let mut request = AllocRequest::new(pages * PAGE_SIZE, PAGE_SIZE, mem_type);
        // Host buffers may be above 4 GiB
        request.window = ram;
        request
    }

    #[test]
    fn the_final_map_lists_itself_and_what_is_handed_over() {
        let (mut map, ram) = state_with_ram(32);
        // Tests run with host addresses, which may be above the limit
        let limit_ok = ram.end() <= ALLOCATION_LIMIT;
        let alloc = |map: &mut MapState, pages, t| {
            let request = request(pages, t, ram);
            map.table.allocate(&request)
        };
        let heap = alloc(&mut map, 2, MemoryType::Loader).unwrap();
        let kernel = alloc(&mut map, 4, MemoryType::Kernel).unwrap();
        let initrd = alloc(&mut map, 3, MemoryType::Initrd).unwrap();
        let cmdline = alloc(&mut map, 1, MemoryType::BootData).unwrap();
        let fb = alloc(&mut map, 2, MemoryType::Framebuffer).unwrap();

        // `finalize` uses the default window; point it at the buffer when
        // the host put it above 4 GiB
        let out = if limit_ok {
            unsafe { map.finalize() }.unwrap()
        } else {
            let out = unsafe { finalize_in(&mut map.table, ram) }.unwrap();
            map.state = State::Frozen;
            out
        };

        let find = |range: PhysRange| {
            out.iter()
                .find(|v| v.base <= range.start() && range.end() <= v.base + v.size)
                .map(|v| v.mem_type)
        };
        assert_eq!(find(heap), Some(BootMemoryType::OsLoaderData));
        assert_eq!(find(kernel), Some(BootMemoryType::Kernel));
        assert_eq!(find(initrd), Some(BootMemoryType::Initrd));
        assert_eq!(find(cmdline), Some(BootMemoryType::BootData));
        assert_eq!(find(fb), Some(BootMemoryType::Framebuffer));
        // The map itself is BootData
        let own =
            PhysRange::from_base_size(out.as_ptr() as u64, core::mem::size_of_val(out) as u64)
                .unwrap()
                .unwrap();
        assert_eq!(find(own), Some(BootMemoryType::BootData));
        // Nothing handed over is reported as reclaimable loader memory
        for handed in [kernel, initrd, cmdline, fb, own] {
            assert!(out.iter().all(|v| {
                v.mem_type != BootMemoryType::OsLoaderData
                    || v.base + v.size <= handed.start()
                    || handed.end() <= v.base
            }));
        }
        // High RAM is listed as available, whole
        assert!(out.iter().any(|v| v.base == 1 << 56
            && v.size == 1 << 30
            && v.mem_type == BootMemoryType::Available
            && v.attributes.contains(BootMemoryAttributes::RAM)));
        // Sorted and without overlaps, neighbours of equal kind merged
        for w in out.windows(2) {
            assert!(w[0].base + w[0].size <= w[1].base);
            assert!(
                w[0].base + w[0].size != w[1].base
                    || w[0].mem_type != w[1].mem_type
                    || w[0].attributes != w[1].attributes
                    || w[1].base == ALLOCATION_LIMIT
            );
        }

        // Frozen: nothing changes any more
        let before: Vec<_> = map.table.as_slice().to_vec();
        assert_eq!(
            map.allocate(&request(1, MemoryType::Loader, ram)),
            Err(MemoryError::Frozen)
        );
        assert_eq!(
            map.free(heap, MemoryType::Loader),
            Err(MemoryFreeError::Frozen)
        );
        assert_eq!(
            map.retype(heap, MemoryType::Loader, MemoryType::BootData),
            Err(MemoryFreeError::Frozen)
        );
        assert_eq!(
            map.update(|t| t.reserve(heap, MemoryType::Reserved, RegionAttrs::empty(), false)),
            Err(MemoryError::Frozen)
        );
        assert_eq!(unsafe { map.finalize() }, Err(MemoryError::Frozen));
        assert_eq!(map.table.as_slice(), &before[..]);
    }

    #[test]
    fn a_failed_finalize_changes_nothing() {
        // No room for the map: the only free pages are taken
        let (mut map, ram) = state_with_ram(4);
        map.table
            .allocate(&request(4, MemoryType::Loader, ram))
            .unwrap();
        let before: Vec<_> = map.table.as_slice().to_vec();
        let result = unsafe { finalize_in(&mut map.table, ram) };
        assert_eq!(result, Err(MemoryError::OutOfMemory));
        assert_eq!(map.table.as_slice(), &before[..]);
        assert_eq!(map.state, State::Ready);
    }

    #[test]
    fn regions_convert_without_truncation() {
        let region = MemoryRegion::new(
            PhysRange::new(0x1_0000_0000, 0x21_0000_0000).unwrap(),
            MemoryType::Reserved,
            RegionAttrs::RAM
                | RegionAttrs::NO_MAP
                | RegionAttrs::RESERVED_MEMORY
                | RegionAttrs::ALLOCATION,
        );
        let out = to_boot_region(&region);
        assert_eq!(out.base, 0x1_0000_0000);
        assert_eq!(out.size, 0x20_0000_0000);
        assert_eq!(out.mem_type, BootMemoryType::Reserved);
        assert_eq!(
            out.attributes,
            BootMemoryAttributes::RAM
                .union(BootMemoryAttributes::NO_MAP)
                .union(BootMemoryAttributes::RESERVED_MEMORY)
        );
    }
}

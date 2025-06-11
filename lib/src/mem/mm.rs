//! Memory Manager
//!
//! TODO: Will rewrite the whole thing later

use super::{MemoryAllocationStrategy, MemoryMapEntry, MemoryType};
use crate::*;
use core::{
    alloc::Layout,
    cell::UnsafeCell,
    cmp,
    ops::{Deref, DerefMut, Range},
    ptr::{NonNull, null_mut},
};
#[allow(unused_imports)]
use minilib::fixedvec::FixedVec;

#[cfg(target_arch = "x86")]
use crate::arch::lomem::LowMemoryManager;

#[cfg(feature = "device_tree")]
use fdt::DeviceTree;

static mut MM: UnsafeCell<MemoryManager> = UnsafeCell::new(MemoryManager::new());

pub struct MemoryManager {
    conventional: MemMapTable,
    himem: Vec<MemoryMapEntry>,
    total_memory_size: usize,
    total_extended_memory_size: usize,
    allocation_strategy: MemoryAllocationStrategy,
}

impl MemoryManager {
    pub const PAGE_SIZE: u64 = 0x1000;

    pub const PAGE_SIZE_M1: u64 = Self::PAGE_SIZE - 1;

    pub const PAGE_MASK: u64 = !(Self::PAGE_SIZE - 1);

    const fn new() -> Self {
        Self {
            conventional: MemMapTable::new(),
            himem: Vec::new(),
            total_memory_size: 0,
            total_extended_memory_size: 0,
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

    #[inline]
    pub unsafe fn init() {
        let page_size_m1 = Self::PAGE_SIZE_M1 as usize;
        let page_mask = Self::PAGE_MASK as usize;

        let info = System::boot_info();
        let start = (info.start_conventional_memory as usize + page_size_m1) & page_mask;
        let end = (info.start_conventional_memory as usize
            + info.conventional_memory_size as usize
            + page_size_m1)
            & page_mask;
        let first_page_size = Self::PAGE_SIZE as usize;
        let first_page_len = first_page_size / core::mem::size_of::<ConventionalMemoryMapEntry>();
        let end = end - first_page_size;
        unsafe {
            let shared = Self::shared_mut();
            shared
                .conventional
                .init(end as *mut ConventionalMemoryMapEntry, first_page_len);
            shared
                .conventional
                .push(ConventionalMemoryMapEntry::new(
                    start,
                    end - start,
                    MemoryType::Available,
                ))
                .unwrap();
            shared
                .conventional
                .push(ConventionalMemoryMapEntry::new(
                    end,
                    first_page_size,
                    MemoryType::Used,
                ))
                .unwrap();
        }
    }

    #[cfg(feature = "device_tree")]
    #[inline]
    pub unsafe fn init_dt(dt: &DeviceTree) {
        let page_size_m1 = Self::PAGE_SIZE_M1 as usize;
        let page_mask = Self::PAGE_MASK as usize;

        let info = System::boot_info();

        if true {
            println!("Early Memory Map from DeviceTree:");
            for item in dt.memory_map().unwrap() {
                println!(
                    "DT MEMMAP: {:08x}-{:08x} {}KB",
                    item.0,
                    item.0 + item.1 - 1,
                    (item.1 + 1023) >> 10,
                );
            }
            for item in dt.header().reserved_maps() {
                println!(
                    "DT RESERVED: {:08x}-{:08x} {}KB",
                    item.0,
                    item.0 + item.1 - 1,
                    (item.1 + 1023) >> 10,
                );
            }
            println!(
                "DT BLOB: {:08x}-{:08x} {}KB",
                dt.range().0 as usize,
                dt.range().0 as usize + dt.range().1 - 1,
                (dt.range().1 + 1023) >> 10,
            );

            println!(
                "Conventional Memory: {:08x}-{:08x} {}KB",
                info.start_conventional_memory as usize,
                info.start_conventional_memory as usize + info.conventional_memory_size as usize
                    - 1,
                (info.conventional_memory_size + 1023) >> 10,
            );
        }

        let start = (info.start_conventional_memory as usize + page_size_m1) & page_mask;
        let end = (info.start_conventional_memory as usize
            + info.conventional_memory_size as usize
            + page_size_m1)
            & page_mask;
        let first_page_size = Self::PAGE_SIZE as usize;
        let first_page_len = first_page_size / core::mem::size_of::<ConventionalMemoryMapEntry>();
        let end = end - first_page_size;
        unsafe {
            let shared = Self::shared_mut();
            shared
                .conventional
                .init(end as *mut ConventionalMemoryMapEntry, first_page_len);
            shared
                .conventional
                .push(ConventionalMemoryMapEntry::new(
                    start,
                    end - start,
                    MemoryType::Available,
                ))
                .unwrap();
            shared
                .conventional
                .push(ConventionalMemoryMapEntry::new(
                    end,
                    first_page_size,
                    MemoryType::Used,
                ))
                .unwrap();

            let range = dt.range();
            Self::register_memmap(
                range.0 as u64..range.0 as u64 + range.1 as u64,
                MemoryType::DeviceTree,
            )
            .unwrap();

            for item in dt.header().reserved_maps() {
                Self::register_memmap(
                    item.0 as u64..item.0 as u64 + item.1 as u64,
                    MemoryType::Reserved,
                )
                .unwrap();
            }
        }
    }

    pub unsafe fn register_memmap(
        range: Range<u64>,
        mem_type: MemoryType,
    ) -> Result<(), MemoryError> {
        let start = range.start & Self::PAGE_MASK;
        let end = (range.end + Self::PAGE_SIZE_M1) & Self::PAGE_MASK;
        if start >= end {
            return Err(MemoryError::InvalidParameter);
        }
        unsafe {
            let shared = Self::shared_mut();
            if range.start >= 0x1_0000_0000 {
                // libpoe itself uses less than 4GB of memory
                let mut index = None;
                let new_item = MemoryMapEntry::new(start, end - start, mem_type);
                for (i, item) in shared.himem.iter().enumerate() {
                    if item.base > new_item.base {
                        index = Some(i);
                        break;
                    }
                }
                if let Some(index) = index {
                    shared.himem.insert(index, new_item);
                } else {
                    shared.himem.push(new_item);
                }

                let mut acc = 0;
                for item in shared.himem.iter() {
                    match item.mem_type {
                        MemoryType::Used | MemoryType::Available => acc += item.size,
                        _ => {}
                    }
                }
                shared.total_extended_memory_size = ((acc + 0xfffff) >> 20) as usize;
            } else if range.end > 0x1_0000_0000 {
                return Err(MemoryError::InvalidParameter);
            } else {
                let new_item = ConventionalMemoryMapEntry::new(
                    start as usize,
                    (end - start) as usize,
                    mem_type,
                );
                let mut index = None;
                for (i, item) in shared.conventional.iter().enumerate() {
                    if item.base() > new_item.base() {
                        index = Some(i);
                        break;
                    }
                }
                if let Some(index) = index {
                    shared
                        .conventional
                        .insert(index, new_item)
                        .map_err(|_| MemoryError::OutOfMemory)?;
                } else {
                    shared
                        .conventional
                        .push(new_item)
                        .map_err(|_| MemoryError::OutOfMemory)?;
                }
            }

            let mut acc = 0;
            if cfg!(target_arch = "x86") {
                acc += 0x10_0000;
            }
            for item in shared.conventional.iter() {
                match item.mem_type() {
                    MemoryType::Used | MemoryType::Available => acc += item.size() as usize,
                    _ => {}
                }
            }
            shared.total_memory_size = acc;
        }
        Ok(())
    }

    #[cfg(target_arch = "x86")]
    #[inline]
    pub fn memory_list<'a>() -> impl Iterator<Item = MemoryMapEntry> + 'a {
        let shared = Self::shared();
        let iter = LowMemoryManager::memory_list();
        iter.chain(
            shared
                .conventional
                .iter()
                .map(|v| MemoryMapEntry::from(v.clone())),
        )
        .chain(shared.himem.iter().cloned())
    }

    #[cfg(not(target_arch = "x86"))]
    #[inline]
    pub fn memory_list<'a>() -> impl Iterator<Item = MemoryMapEntry> + 'a {
        let shared = Self::shared();
        let iter = shared
            .conventional
            .iter()
            .map(|v| MemoryMapEntry::from(v.clone()));
        iter.chain(shared.himem.iter().cloned())
    }

    #[inline]
    pub fn total_memory_size() -> usize {
        let shared = Self::shared();
        shared.total_memory_size
    }

    #[inline]
    pub fn total_extended_memory_size() -> usize {
        let shared = Self::shared();
        shared.total_extended_memory_size
    }

    pub fn free_memory_count() -> usize {
        let shared = Self::shared();
        let mut acc = 0;
        for item in shared.conventional.iter() {
            match item.mem_type() {
                MemoryType::Available => acc += item.size() as usize,
                _ => {}
            }
        }
        acc
    }

    pub fn max_free_memory_size() -> usize {
        let shared = Self::shared();
        let mut max = 0;
        for item in shared.conventional.iter() {
            match item.mem_type() {
                MemoryType::Available => {
                    if item.size() > max {
                        max = item.size();
                    }
                }
                _ => {}
            }
        }
        max as usize
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

    #[must_use]
    pub fn zalloc(
        layout: Layout,
        desired_addr: Option<NonNullPhysicalAddress>,
        mem_type: MemoryType,
        strategy: Option<MemoryAllocationStrategy>,
    ) -> Result<*mut u8, MemoryError> {
        if mem_type == MemoryType::Available
            || layout.size() > i32::MAX as usize
            || layout.align() > i32::MAX as usize
        {
            return Err(MemoryError::InvalidParameter);
        }

        let shared = unsafe { Self::shared_mut() };
        without_interrupts!(unsafe {
            shared._zalloc(
                layout,
                desired_addr,
                mem_type,
                strategy.unwrap_or(shared.allocation_strategy),
            )
        })
    }

    pub unsafe fn zfree(ptr: *mut u8, layout: Layout) -> Result<(), MemoryFreeError> {
        if ptr == null_mut() {
            // do nothing
            return Ok(());
        }

        let shared = unsafe { Self::shared_mut() };
        without_interrupts!(unsafe { shared._zfree(ptr, layout) })
    }

    #[inline]
    unsafe fn _zalloc(
        &mut self,
        layout: Layout,
        desired_addr: Option<NonNullPhysicalAddress>,
        mem_type: MemoryType,
        strategy: MemoryAllocationStrategy,
    ) -> Result<*mut u8, MemoryError> {
        let size = (layout.size() + Self::PAGE_SIZE_M1 as usize) & Self::PAGE_MASK as usize;
        let align = (layout.align()).max(Self::PAGE_SIZE as usize);
        let align_m1 = align - 1;
        let align_mask = !align_m1;

        let mut found = None;
        if let Some(desired_addr) = desired_addr {
            let desired_addr = desired_addr.get().as_usize();
            for (index, item) in self.conventional.iter_mut().enumerate() {
                match item.mem_type() {
                    MemoryType::Available => {
                        if item.range().contains(&desired_addr) {
                            if item.range().end >= desired_addr + size {
                                found = Some((index, item, desired_addr, size));
                                break;
                            } else {
                                return Err(MemoryError::OutOfMemory);
                            }
                        }
                    }
                    _ => {}
                }
            }
        } else {
            match strategy {
                MemoryAllocationStrategy::FirstFit | MemoryAllocationStrategy::BestFit => {
                    for (index, item) in self.conventional.iter_mut().enumerate() {
                        match item.mem_type() {
                            MemoryType::Available => {
                                let start = (item.base() + align_m1) & align_mask;
                                let end = item.range().end;
                                let end_fixed = start + size;
                                if end >= end_fixed {
                                    if strategy == MemoryAllocationStrategy::BestFit {
                                        if found.as_ref().map(|v| v.1.size()).unwrap_or(0)
                                            < item.size()
                                        {
                                            found = Some((index, item, start, size));
                                        }
                                    } else {
                                        found = Some((index, item, start, size));
                                    }
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                MemoryAllocationStrategy::LastFit => {
                    for (index, item) in self.conventional.iter_mut().enumerate().rev() {
                        match item.mem_type() {
                            MemoryType::Available => {
                                let end = item.range().end;
                                let start = (end - size) & align_mask;
                                if start >= item.base() {
                                    found = Some((index, item, start, size));
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        let Some(found) = found else {
            return Err(MemoryError::OutOfMemory);
        };

        let mut target_index = found.0;
        let target = found.1;
        let new_item = ConventionalMemoryMapEntry::new(found.2, found.3, mem_type);

        let extra_item = (new_item.range().end < target.range().end).then(|| {
            ConventionalMemoryMapEntry::new(
                new_item.range().end,
                target.range().end - new_item.range().end,
                MemoryType::Available,
            )
        });

        if new_item.base() > target.base() {
            target.set_size(new_item.base() - target.base());
            target_index += 1;
            self.conventional
                .insert(target_index, new_item.clone())
                .unwrap();
        } else {
            *target = new_item.clone();
        }
        if let Some(extra_item) = extra_item {
            target_index += 1;
            self.conventional.insert(target_index, extra_item).unwrap();
        }

        let p = new_item.base() as *mut u8;
        unsafe {
            p.write_bytes(0, new_item.size() as usize);
        }
        Ok(p)
    }

    #[inline]
    unsafe fn _zfree(&mut self, ptr: *mut u8, layout: Layout) -> Result<(), MemoryFreeError> {
        let size =
            (layout.size() as usize + Self::PAGE_SIZE_M1 as usize) & Self::PAGE_MASK as usize;
        let temp = ConventionalMemoryMapEntry::new(ptr as usize, size, MemoryType::Used);
        for item in self.conventional.iter_mut() {
            if item.range() == temp.range() {
                if item.mem_type() == MemoryType::Available {
                    return Err(MemoryFreeError::DoubleFree);
                } else {
                    item.set_mem_type(MemoryType::Available);
                    return Ok(());
                }
            }
        }
        Err(MemoryFreeError::InvalidPointer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    InvalidParameter,
    OutOfMemory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryFreeError {
    InvalidParameter,
    InvalidPointer,
    DoubleFree,
}

#[repr(C)]
#[derive(Clone, PartialEq, Eq)]
pub struct ConventionalMemoryMapEntry {
    base: usize,
    size_type: usize,
}

impl ConventionalMemoryMapEntry {
    #[inline]
    const fn new(base: usize, size: usize, mem_type: MemoryType) -> Self {
        Self {
            base,
            size_type: (size & MemoryManager::PAGE_MASK as usize) | mem_type as usize,
        }
    }

    #[inline]
    pub fn base(&self) -> usize {
        self.base
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.size_type & MemoryManager::PAGE_MASK as usize
    }

    #[inline]
    pub fn set_size(&mut self, new_size: usize) {
        self.size_type = (new_size & MemoryManager::PAGE_MASK as usize) | self.mem_type() as usize;
    }

    #[inline]
    pub fn set_mem_type(&mut self, mem_type: MemoryType) {
        self.size_type = (self.size_type & MemoryManager::PAGE_MASK as usize) | mem_type as usize;
    }

    #[inline]
    pub fn mem_type(&self) -> MemoryType {
        match self.size_type & 0xff {
            0 => MemoryType::Used,
            1 => MemoryType::Available,
            2 => MemoryType::Reserved,
            3 => MemoryType::AcpiReclaim,
            4 => MemoryType::AcpiNvs,
            5 => MemoryType::DeviceTree,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn range(&self) -> Range<usize> {
        Range {
            start: self.base,
            end: self.base + self.size(),
        }
    }
}

impl PartialOrd for ConventionalMemoryMapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.base.partial_cmp(&other.base)
    }
}

impl Ord for ConventionalMemoryMapEntry {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.base.cmp(&other.base)
    }
}

impl From<ConventionalMemoryMapEntry> for MemoryMapEntry {
    fn from(v: ConventionalMemoryMapEntry) -> Self {
        MemoryMapEntry::new(v.base as u64, v.size() as u64, v.mem_type())
    }
}

impl core::fmt::Debug for ConventionalMemoryMapEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("LowMemoryMapEntry")
            .field(&self.base)
            .field(&self.size())
            .field(&self.mem_type())
            .finish()
    }
}

pub struct MemMapTable {
    ptr: NonNull<ConventionalMemoryMapEntry>,
    len: usize,
    cap: usize,
}

impl MemMapTable {
    const SIZE_OF_ELEMENT: usize = core::mem::size_of::<ConventionalMemoryMapEntry>();

    #[inline]
    pub const fn new() -> Self {
        Self {
            ptr: NonNull::dangling(),
            len: 0,
            cap: 0,
        }
    }

    #[inline]
    pub unsafe fn init(
        &mut self,
        first_page: *mut ConventionalMemoryMapEntry,
        first_page_size: usize,
    ) {
        self.ptr = unsafe { NonNull::new_unchecked(first_page) };
        self.cap = first_page_size / Self::SIZE_OF_ELEMENT;
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn as_slice(&self) -> &[ConventionalMemoryMapEntry] {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [ConventionalMemoryMapEntry] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }

    pub fn push(
        &mut self,
        value: ConventionalMemoryMapEntry,
    ) -> Result<(), ConventionalMemoryMapEntry> {
        if self.len >= self.cap {
            return Err(value);
        }
        unsafe {
            core::ptr::write(self.ptr.as_ptr().add(self.len), value);
        }
        self.len += 1;
        Ok(())
    }

    pub fn insert(
        &mut self,
        index: usize,
        value: ConventionalMemoryMapEntry,
    ) -> Result<(), ConventionalMemoryMapEntry> {
        if self.len >= self.cap || index >= self.len {
            return Err(value);
        }
        unsafe {
            core::ptr::copy(
                self.ptr.as_ptr().add(index),
                self.ptr.as_ptr().add(index + 1),
                self.len - index,
            );
            core::ptr::write(self.ptr.as_ptr().add(index), value);
        }
        self.len += 1;
        Ok(())
    }
}

impl Deref for MemMapTable {
    type Target = [ConventionalMemoryMapEntry];

    #[inline]
    fn deref(&self) -> &[ConventionalMemoryMapEntry] {
        self.as_slice()
    }
}

impl DerefMut for MemMapTable {
    #[inline]
    fn deref_mut(&mut self) -> &mut [ConventionalMemoryMapEntry] {
        self.as_mut_slice()
    }
}

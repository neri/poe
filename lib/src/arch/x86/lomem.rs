//! Real Mode Memory Manager

use super::bits::AtomicBitArray;
use crate::*;
use core::{cell::UnsafeCell, num::NonZeroU16, ops::Range};
use mem::{MemoryMapEntry, MemoryType};
use x86::prot::{Limit16, Linear32, Selector};

static mut LMM: UnsafeCell<LoMemoryManager> = UnsafeCell::new(LoMemoryManager::new());

/// Low Memory Manager
///
/// This manager is used to manage the first 1MB of memory.
pub struct LoMemoryManager {
    free_bitmap: AtomicBitArray<8>,
    reserved_bitmap: AtomicBitArray<8>,
    acpi_reclaim_bitmap: AtomicBitArray<8>,
    acpi_nvs_bitmap: AtomicBitArray<8>,
}

#[allow(unused)]
impl LoMemoryManager {
    const PAGE_SIZE: usize = 0x1000;

    const PAGE_SIZE_M1: usize = Self::PAGE_SIZE - 1;

    const fn new() -> Self {
        Self {
            free_bitmap: AtomicBitArray::new(),
            reserved_bitmap: AtomicBitArray::new(),
            acpi_reclaim_bitmap: AtomicBitArray::new(),
            acpi_nvs_bitmap: AtomicBitArray::new(),
        }
    }

    #[inline]
    unsafe fn shared_mut<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut LMM)).get_mut() }
    }

    #[inline]
    fn shared<'a>() -> &'a Self {
        unsafe { &*(&*(&raw const LMM)).get() }
    }

    pub(crate) unsafe fn init() {
        unsafe {
            let shared = Self::shared_mut();

            // 00_0000-00_0fff reserved in x86 platform
            shared.reserved_bitmap.set(0);

            let info = System::boot_info();
            // 4KB / 16 = 256
            for i in 1..(info.x86_real_memory_size / 256) {
                shared.free_bitmap.set(i as usize);
            }

            Self::reserve(
                (info.x86_real_memory_size as usize * 16)..0x10_0000,
                MemoryType::Reserved,
            )
            .unwrap();
        }
    }

    /// Reserve pages
    pub unsafe fn reserve(range: Range<usize>, mem_type: MemoryType) -> Result<(), ReserveError> {
        if range.start == 0 || range.end > 0x10_0000 || range.start >= range.end {
            return Err(ReserveError::InvalidParameter);
        }
        let mut reserved = false;
        let mut acpi_reclaim = false;
        let mut acpi_nvs = false;
        match mem_type {
            MemoryType::Available => return Err(ReserveError::InvalidParameter),
            MemoryType::Used => {}
            MemoryType::Reserved => reserved = true,
            MemoryType::AcpiReclaim => acpi_reclaim = true,
            MemoryType::AcpiNvs => acpi_nvs = true,
            MemoryType::DeviceTree => reserved = true,
        }
        let fixed_range =
            (range.start / Self::PAGE_SIZE)..((range.end + Self::PAGE_SIZE_M1) / Self::PAGE_SIZE);
        let shared = unsafe { Self::shared_mut() };
        for i in fixed_range {
            shared.free_bitmap.reset(i);
            if reserved {
                shared.reserved_bitmap.set(i);
            } else if acpi_reclaim {
                shared.acpi_reclaim_bitmap.set(i);
            } else if acpi_nvs {
                shared.acpi_nvs_bitmap.set(i);
            }
        }
        Ok(())
    }

    pub fn alloc_page_checked() -> Option<ManagedLowMemory> {
        unsafe {
            let shared = Self::shared_mut();
            for i in 1..256 {
                if shared.free_bitmap.fetch_reset_unchecked(i) {
                    return Some(ManagedLowMemory::new(
                        i as u16 * 256,
                        NonZeroU16::new(Self::PAGE_SIZE_M1 as u16).unwrap(),
                    ));
                }
            }
        }
        None
    }

    #[inline]
    #[track_caller]
    pub fn alloc_page() -> ManagedLowMemory {
        Self::alloc_page_checked().expect("Out of low memory")
    }

    unsafe fn free_page(page: &ManagedLowMemory) {
        let shared = unsafe { Self::shared_mut() };
        let page_index = page.base().0 as usize / Self::PAGE_SIZE;
        for i in 0..=(page.limit().0 as usize / Self::PAGE_SIZE) {
            shared.free_bitmap.set(page_index + i);
        }
    }

    #[inline]
    pub fn memory_list<'a>() -> impl Iterator<Item = MemoryMapEntry> + 'a {
        let shared = Self::shared();
        LowMemoryIter {
            instance: shared,
            current: 0,
            prev_attr: None,
            terminated: false,
        }
    }

    pub fn free_memory_size() -> usize {
        let shared = Self::shared();
        shared.free_bitmap.count() * Self::PAGE_SIZE
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveError {
    InvalidParameter,
}

#[repr(C)]
pub struct ManagedLowMemory {
    limit: NonZeroU16,
    base_para: u16,
}

#[allow(unused)]
impl ManagedLowMemory {
    #[inline]
    const unsafe fn new(base_para: u16, limit: NonZeroU16) -> Self {
        Self { base_para, limit }
    }

    #[inline]
    pub const fn base(&self) -> Linear32 {
        Linear32((self.base_para as u32) * 16)
    }

    #[inline]
    pub fn sel(&self) -> Selector {
        Selector(self.base_para)
    }

    #[inline]
    pub const fn limit(&self) -> Limit16 {
        Limit16(self.limit.get())
    }

    #[inline]
    pub fn as_slice<'a>(&self) -> &'a mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.base().0 as usize as *mut u8,
                self.limit().0 as usize + 1,
            )
        }
    }
}

impl Drop for ManagedLowMemory {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            LoMemoryManager::free_page(self);
        }
    }
}

struct LowMemoryIter<'a> {
    instance: &'a LoMemoryManager,
    current: usize,
    prev_attr: Option<(usize, MemoryType)>,
    terminated: bool,
}

impl Iterator for LowMemoryIter<'_> {
    type Item = MemoryMapEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.terminated {
            return None;
        }
        let mut current = self.current;
        loop {
            if current >= 256 {
                self.terminated = true;

                if let Some((prev, prev_attr)) = self.prev_attr {
                    self.prev_attr = None;
                    return Some(MemoryMapEntry::new(
                        (prev * LoMemoryManager::PAGE_SIZE) as u64,
                        ((current - prev) * LoMemoryManager::PAGE_SIZE) as u64,
                        prev_attr,
                    ));
                } else {
                    return None;
                }
            }
            let free = self.instance.free_bitmap.fetch(current);
            let reserved = self.instance.reserved_bitmap.fetch(current);
            let acpi_reclaim = self.instance.acpi_reclaim_bitmap.fetch(current);
            let acpi_nvs = self.instance.acpi_nvs_bitmap.fetch(current);
            let current_type = if acpi_reclaim {
                MemoryType::AcpiReclaim
            } else if acpi_nvs {
                MemoryType::AcpiNvs
            } else if reserved {
                MemoryType::Reserved
            } else if free {
                MemoryType::Available
            } else {
                MemoryType::Used
            };
            if let Some((prev, prev_type)) = self.prev_attr {
                if prev_type != current_type {
                    self.prev_attr = Some((current, current_type));
                    current += 1;
                    self.current = current;
                    return Some(MemoryMapEntry::new(
                        (prev * LoMemoryManager::PAGE_SIZE) as u64,
                        ((current - prev - 1) * LoMemoryManager::PAGE_SIZE) as u64,
                        prev_type,
                    ));
                }
            } else {
                self.prev_attr = Some((current, current_type));
            }
            current += 1;
        }
    }
}

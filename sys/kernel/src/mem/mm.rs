// Memory Manager

use super::slab::SlabAllocator;
use super::string::StringBuffer;
use crate::arch::cpu::Cpu;
use crate::*;
use bitflags::*;
use core::alloc::Layout;
use core::num::*;
use toeboot::*;

static mut MM: MemoryManager = MemoryManager::new();

pub struct MemoryManager {
    total_memory_size: usize,
    reserved_memory_size: usize,
    dummy_size: usize,
    n_free: usize,
    pairs: [MemFreePair; Self::MAX_FREE_PAIRS],
    slab: Option<SlabAllocator>,
}

impl MemoryManager {
    const MAX_FREE_PAIRS: usize = 1024;
    pub const PAGE_SIZE_MIN: usize = 0x1000;

    const fn new() -> Self {
        Self {
            total_memory_size: 0,
            reserved_memory_size: 0,
            dummy_size: 0,
            n_free: 0,
            pairs: [MemFreePair::empty(); Self::MAX_FREE_PAIRS],
            slab: None,
        }
    }

    pub(crate) unsafe fn init_first(info: &BootInfo) {
        let shared = Self::shared();

        shared.total_memory_size = info.total_memory_size as usize;
        shared.reserved_memory_size = info.reserved_memory_size as usize;
        shared.pairs[0] = MemFreePair {
            base: info.smap.0 as usize,
            size: info.smap.1 as usize,
        };
        shared.n_free = 1;

        shared.slab = Some(SlabAllocator::new());

        // todo!();
    }

    pub(crate) unsafe fn late_init() {
        //
    }

    #[inline]
    fn shared<'a>() -> &'a mut Self {
        unsafe { &mut MM }
    }

    #[inline]
    pub fn total_memory_size() -> usize {
        let shared = Self::shared();
        shared.total_memory_size
    }

    #[inline]
    pub fn reserved_memory_size() -> usize {
        let shared = Self::shared();
        shared.reserved_memory_size
    }

    #[inline]
    pub fn free_memory_size() -> usize {
        let shared = Self::shared();
        let mut total = shared.dummy_size;
        total += shared
            .slab
            .as_ref()
            .map(|v| v.free_memory_size())
            .unwrap_or(0);
        total += shared.pairs[..shared.n_free]
            .iter()
            .fold(0, |v, i| v + i.size);
        total
    }

    #[inline]
    pub unsafe fn direct_map(
        base: usize,
        _size: usize,
        _prot: MProtect,
    ) -> Result<NonZeroUsize, AllocationError> {
        NonZeroUsize::new(base).ok_or(AllocationError::InvalidArgument)
    }

    /// Allocate static pages
    unsafe fn static_alloc(layout: Layout) -> Result<NonZeroUsize, AllocationError> {
        let shared = Self::shared();

        let align_m1 = Self::PAGE_SIZE_MIN - 1;
        let size = (layout.size() + align_m1) & !(align_m1);
        for i in 0..shared.n_free {
            let free_pair = &mut shared.pairs[i];
            if free_pair.size >= size {
                let ptr = free_pair.base;
                free_pair.base += size;
                free_pair.size -= size;
                return Ok(NonZeroUsize::new_unchecked(ptr));
            }
        }
        Err(AllocationError::OutOfMemory)
    }

    /// Allocate kernel memory
    pub unsafe fn zalloc(layout: Layout) -> Result<NonZeroUsize, AllocationError> {
        Cpu::without_interrupts(|| {
            let shared = Self::shared();
            if let Some(slab) = &shared.slab {
                let r = slab.alloc(layout);
                match r {
                    Err(AllocationError::Unsupported) => (),
                    _ => return r,
                }
            }
            Self::static_alloc(layout)
        })
    }

    /// Deallocate kernel memory
    pub unsafe fn zfree(
        base: Option<NonZeroUsize>,
        layout: Layout,
    ) -> Result<(), DeallocationError> {
        if let Some(base) = base {
            Cpu::without_interrupts(|| {
                let ptr = base.get() as *mut u8;
                ptr.write_bytes(0xCC, layout.size());

                let shared = Self::shared();
                if let Some(slab) = &shared.slab {
                    match slab.free(base, layout) {
                        Ok(_) => Ok(()),
                        Err(_) => {
                            shared.dummy_size += layout.size();
                            Ok(())
                        }
                    }
                } else {
                    shared.dummy_size += layout.size();
                    Ok(())
                }
            })
        } else {
            Ok(())
        }
    }

    #[allow(dead_code)]
    pub fn statistics_slab(sb: &mut StringBuffer) {
        let shared = Self::shared();
        for slab in shared.slab.as_ref().unwrap().statistics() {
            writeln!(sb, "Slab {:4}: {:3} / {:3}", slab.0, slab.1, slab.2).unwrap();
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct MemFreePair {
    base: usize,
    size: usize,
}

impl MemFreePair {
    const fn empty() -> Self {
        Self { base: 0, size: 0 }
    }
}

bitflags! {
    pub struct MProtect: usize {
        const READ  = 0x1;
        const WRITE = 0x2;
        const EXEC  = 0x4;
        const NONE  = 0x0;
    }
}

#[derive(Debug, Copy, Clone)]
pub enum AllocationError {
    Unexpected,
    OutOfMemory,
    InvalidArgument,
    Unsupported,
}

#[derive(Debug, Copy, Clone)]
pub enum DeallocationError {
    Unexpected,
    InvalidArgument,
    Unsupported,
}

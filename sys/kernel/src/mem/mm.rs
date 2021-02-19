// Memory Manager
use crate::arch::cpu::Cpu;
use core::alloc::Layout;
use core::num::*;
use toeboot::*;

static mut MM: MemoryManager = MemoryManager::new();

pub struct MemoryManager {
    total_memory_size: usize,
    n_free: usize,
    pairs: [MemFreePair; Self::MAX_FREE_PAIRS],
}

impl MemoryManager {
    const MAX_FREE_PAIRS: usize = 4096;
    const PAGE_SIZE_MIN: usize = 0x1000;

    const fn new() -> Self {
        Self {
            total_memory_size: 0,
            n_free: 0,
            pairs: [MemFreePair::empty(); Self::MAX_FREE_PAIRS],
        }
    }

    pub(crate) unsafe fn init_first(info: &BootInfo) {
        let shared = Self::shared();

        shared.total_memory_size = (info.smap.0 + info.smap.1) as usize;
        shared.pairs[0] = MemFreePair {
            base: info.smap.0 as usize,
            size: info.smap.1 as usize,
        };
        shared.n_free = 1;
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
    pub fn free_memory_size() -> usize {
        let shared = Self::shared();
        shared.pairs[..shared.n_free]
            .into_iter()
            .fold(0, |v, i| v + i.size)
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
            // let shared = Self::shared();
            // if let Some(slab) = &shared.slab {
            //     match slab.alloc(layout) {
            //         Ok(result) => return Ok(result),
            //         Err(AllocationError::Unsupported) => (),
            //         Err(err) => return Err(err),
            //     }
            // }
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

                // let shared = Self::shared();
                // if let Some(slab) = &shared.slab {
                //     match slab.free(base, layout) {
                //         Ok(_) => (),
                //         Err(err) => return Err(err),
                //     }
                // }
            })
        }
        Ok(())
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

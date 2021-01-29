// Memory Manager
use crate::arch::cpu::Cpu;
use bootprot::*;
use core::alloc::Layout;
use core::num::*;

static mut MM: MemoryManager = MemoryManager::new();

pub struct MemoryManager {
    total_memory_size: usize,
    page_size_min: usize,
    static_start: usize,
    static_free: usize,
    static_end: usize,
}

impl MemoryManager {
    const fn new() -> Self {
        Self {
            total_memory_size: 0,
            page_size_min: 0x1000,
            static_start: 0,
            static_free: 0,
            static_end: 0,
        }
    }

    pub(crate) unsafe fn init_first(info: &BootInfo) {
        let shared = Self::shared();
        shared.total_memory_size = (info.smap.0 + info.smap.1) as usize;
        shared.static_start = info.kernel_end as usize;
        shared.static_free = (info.smap.0 - info.kernel_end) as usize;
        shared.static_end = shared.static_start + shared.static_free;
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
    pub fn page_size_min() -> usize {
        let shared = Self::shared();
        shared.page_size_min
    }

    /// Allocate static pages
    unsafe fn static_alloc(layout: Layout) -> Result<NonZeroUsize, AllocationError> {
        // let shared = Self::shared();
        // let page_mask = shared.page_size_min() - 1;
        // let align = usize::max(layout.align(), 16);
        // let ptr = (shared.static_start + align - 1) & !align;
        // let size = layout.size();
        // let new_start = ptr + size;
        // if new_start < shared.static_end {
        //     shared.static_start = new_start;
        //     Ok(NonZeroUsize::new_unchecked(ptr))
        // } else {
        //     Err(AllocationError::OutOfMemory)
        // }
        Err(AllocationError::Unexpected)
    }

    /// Allocate kernel memory
    pub unsafe fn zalloc_layout(layout: Layout) -> Result<NonZeroUsize, AllocationError> {
        Cpu::without_interrupts(|| {
            let shared = Self::shared();
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

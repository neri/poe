//! Global Allocator
#![cfg(not(test))]

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;

use super::{MemoryFreeError, MemoryManager, MemoryType};

#[global_allocator]
static ALLOC: CustomAlloc = CustomAlloc::new();

pub struct CustomAlloc;

impl CustomAlloc {
    const fn new() -> Self {
        CustomAlloc {}
    }
}

unsafe impl GlobalAlloc for CustomAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        MemoryManager::zalloc(layout, None, MemoryType::Loader, None).unwrap_or(null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        match unsafe { MemoryManager::zfree(ptr, layout) } {
            // After the final map, freed memory stays `Loader` and is
            // reclaimed by the next OS with the rest of MiniOS.
            Ok(()) | Err(MemoryFreeError::Frozen) => {}
            Err(err) => panic!("dealloc {:p} {:?}: {:?}", ptr, layout, err),
        }
    }
}

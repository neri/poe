//! Global Allocator

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;

use super::{MemoryManager, MemoryType};

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
        MemoryManager::zalloc(layout, None, MemoryType::Used, None).unwrap_or(null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            MemoryManager::zfree(ptr, layout).unwrap();
        }
    }
}

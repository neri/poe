//! Global Allocator

use super::{MemoryManager, MemoryType};
use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::null_mut,
};

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

// #[alloc_error_handler]
// fn alloc_error_handler(layout: Layout) -> ! {
//     panic!("allocation error: {:?}", layout)
// }

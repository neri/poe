//! Owned DMA memory for the identity-mapped QEMU virt machines.
use alloc::alloc::{alloc_zeroed, dealloc};
use alloc::sync::Arc;
use core::alloc::Layout;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicBool, Ordering};

use super::barrier;

pub struct Dma {
    cpu: NonNull<u8>,
    device: u64,
    layout: Layout,
    owner: Option<Arc<AtomicBool>>,
}

// The allocation is uniquely owned; synchronization with the device is
// performed explicitly at each ownership transition.
unsafe impl Send for Dma {}

impl Dma {
    pub fn new(len: usize, align: usize) -> Result<Self, &'static str> {
        let layout = Layout::from_size_align(len.max(1), align).map_err(|_| "DMA layout")?;
        let cpu = NonNull::new(unsafe { alloc_zeroed(layout) }).ok_or("DMA allocation")?;
        let device = cpu.as_ptr() as u64;
        // The current QEMU virt platforms use identity DMA and a 32-bit heap.
        if !cfg!(test)
            && device
                .checked_add(len as u64)
                .is_none_or(|end| end > u32::MAX as u64 + 1)
        {
            unsafe { dealloc(cpu.as_ptr(), layout) };
            return Err("DMA address outside guest RAM window");
        }
        Ok(Self {
            cpu,
            device,
            layout,
            owner: None,
        })
    }

    /// Keeps the allocation alive if a device fails to acknowledge reset.
    pub fn with_owner(mut self, owner: Arc<AtomicBool>) -> Self {
        self.owner = Some(owner);
        self
    }

    pub fn addr(&self) -> u64 {
        self.device
    }
    pub fn len(&self) -> usize {
        self.layout.size()
    }
    pub fn as_ptr(&self) -> *mut u8 {
        self.cpu.as_ptr()
    }
    pub fn bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.as_ptr(), self.len()) }
    }

    pub fn release(&self) {
        self.release_range(0, self.len());
    }
    pub fn release_range(&self, offset: usize, len: usize) {
        if offset > self.len() || len > self.len() - offset {
            return;
        }
        #[cfg(all(target_arch = "aarch64", not(test)))]
        unsafe {
            crate::arch::cache::dcache_clean(self.as_ptr() as usize + offset, len)
        };
        barrier::device();
    }
    pub fn acquire(&self) {
        barrier::device();
        #[cfg(all(target_arch = "aarch64", not(test)))]
        unsafe {
            crate::arch::cache::dcache_invalidate(self.as_ptr() as usize, self.len())
        };
    }
}

impl Drop for Dma {
    fn drop(&mut self) {
        if self
            .owner
            .as_ref()
            .is_some_and(|owner| owner.load(Ordering::Acquire))
        {
            return;
        }
        unsafe { dealloc(self.cpu.as_ptr(), self.layout) }
    }
}

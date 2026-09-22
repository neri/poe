//! What the xHCI driver needs from the platform it runs on.
//!
//! Everything board specific stays behind this trait: the driver never sees a
//! PCI configuration space, a mailbox, or a BCM2711 register.  A host that
//! reaches its xHCI some other way only has to implement [`XhciEnv`].
//!
//! The DMA contract is deliberately explicit.  A [`Dma`] region owns its
//! memory for as long as the controller may reference it; the driver drops one
//! only after it has confirmed the controller has stopped using it.

use core::marker::PhantomData;
use core::ptr::NonNull;

/// A block of memory both the CPU and the controller can reach.
///
/// `device` is the address to program into a register or a TRB, which is not
/// necessarily `cpu`: a host bridge may place RAM at a different address on
/// the bus side.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmaAllocation {
    pub cpu: NonNull<u8>,
    pub device: u64,
    pub len: usize,
    pub align: usize,
}

/// Services the xHCI driver needs from its platform.
pub trait XhciEnv {
    /// A monotonic microsecond counter.  Used only for deadlines, so its
    /// origin does not matter as long as it does not go backwards.
    fn now_us(&self) -> u64;

    /// Allocates `len` zeroed bytes aligned to `align`, reachable by the
    /// controller's DMA.  `align` is always a power of two and at most 4096.
    ///
    /// The driver additionally relies on a region of at most 4096 bytes never
    /// crossing a 64 KiB boundary, which page-granular allocation satisfies.
    fn alloc_dma(&self, len: usize, align: usize) -> Option<DmaAllocation>;

    /// Releases a region previously returned by [`Self::alloc_dma`].
    ///
    /// # Safety
    /// The controller must no longer reference the region.
    unsafe fn free_dma(&self, allocation: DmaAllocation);

    /// Orders CPU writes to DMA memory before the following MMIO write, so a
    /// doorbell never reaches the controller ahead of the TRB it announces.
    fn write_barrier(&self);

    /// Orders an MMIO read or a device write to DMA memory before the CPU
    /// reads that memory.
    fn read_barrier(&self);

    /// Called from the driver's foreground poll, so a platform that counts
    /// interrupt entries can tell that the foreground is still running.
    ///
    /// A platform with no interrupt handler leaves this alone.
    fn note_foreground_progress(&self) {}

    /// True once the platform has given the interrupt up as unserviceable and
    /// the driver should go back to polling.
    fn interrupt_stalled(&self) -> bool {
        false
    }
}

/// An owned DMA region, typed as a `T`.
///
/// Dropping it calls back into the environment, so the driver must not drop
/// one while the controller might still walk it.
pub struct Dma<'e, E: XhciEnv + ?Sized, T> {
    env: &'e E,
    allocation: DmaAllocation,
    _marker: PhantomData<*mut T>,
}

impl<'e, E: XhciEnv + ?Sized, T> Dma<'e, E, T> {
    /// Allocates room for `count` values of `T`, zeroed, aligned to `align`
    /// (raised to `align_of::<T>()`).
    pub fn new(env: &'e E, count: usize, align: usize) -> Option<Self> {
        let len = core::mem::size_of::<T>().checked_mul(count)?;
        if len == 0 {
            return None;
        }
        let align = align.max(core::mem::align_of::<T>());
        let allocation = env.alloc_dma(len, align)?;
        Some(Self {
            env,
            allocation,
            _marker: PhantomData,
        })
    }

    #[inline]
    pub fn device_address(&self) -> u64 {
        self.allocation.device
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.allocation.len / core::mem::size_of::<T>()
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut T {
        self.allocation.cpu.as_ptr().cast()
    }

    /// The device address of the `index`-th element.
    #[inline]
    pub fn element_address(&self, index: usize) -> u64 {
        self.allocation.device + (index * core::mem::size_of::<T>()) as u64
    }

    /// Reads the `index`-th element with a volatile load.
    ///
    /// Volatile because the controller writes these behind the compiler's
    /// back; a plain read may be hoisted out of a polling loop.
    #[inline]
    pub fn read(&self, index: usize) -> T {
        assert!(index < self.len());
        unsafe { self.as_ptr().add(index).read_volatile() }
    }

    /// Writes the `index`-th element with a volatile store.
    #[inline]
    pub fn write(&self, index: usize, value: T) {
        assert!(index < self.len());
        unsafe { self.as_ptr().add(index).write_volatile(value) }
    }
}

impl<E: XhciEnv + ?Sized> Dma<'_, E, u8> {
    /// Copies the first `dst.len()` bytes out of the buffer.
    ///
    /// Eight bytes per volatile load rather than one: with the data cache
    /// off, as on the Raspberry Pi here, every load is its own trip to DRAM.
    pub fn copy_to_slice(&self, dst: &mut [u8]) {
        assert!(dst.len() <= self.len());
        let base = self.as_ptr();
        let words = if base as usize % 8 == 0 {
            dst.len() / 8
        } else {
            0
        };
        for (i, chunk) in dst[..words * 8].chunks_exact_mut(8).enumerate() {
            let word = unsafe { base.cast::<u64>().add(i).read_volatile() };
            chunk.copy_from_slice(&word.to_ne_bytes());
        }
        for (offset, byte) in dst.iter_mut().enumerate().skip(words * 8) {
            *byte = unsafe { base.add(offset).read_volatile() };
        }
    }

    /// Copies `src` into the start of the buffer, eight bytes per volatile
    /// store where it can.
    pub fn copy_from_slice(&self, src: &[u8]) {
        assert!(src.len() <= self.len());
        let base = self.as_ptr();
        let words = if base as usize % 8 == 0 {
            src.len() / 8
        } else {
            0
        };
        for (i, chunk) in src[..words * 8].chunks_exact(8).enumerate() {
            let word = u64::from_ne_bytes(chunk.try_into().unwrap());
            unsafe { base.cast::<u64>().add(i).write_volatile(word) };
        }
        for (offset, byte) in src.iter().enumerate().skip(words * 8) {
            unsafe { base.add(offset).write_volatile(*byte) };
        }
    }
}

impl<E: XhciEnv + ?Sized, T> Drop for Dma<'_, E, T> {
    fn drop(&mut self) {
        unsafe { self.env.free_dma(self.allocation) }
    }
}

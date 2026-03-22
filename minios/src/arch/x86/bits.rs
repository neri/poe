use core::arch::asm;
use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::mem::transmute;
use core::ops::{Deref, DerefMut};

#[derive(Debug, Clone)]
pub struct BitArray<const N: usize> {
    inner: [u32; N],
}

pub struct AtomicBitArray<const N: usize> {
    _inner: UnsafeCell<[u32; N]>,
}

#[allow(unused)]
impl<const N: usize> BitArray<N> {
    #[inline]
    pub const fn new() -> Self {
        Self { inner: [0; N] }
    }

    /// Return the number of bits in the array.
    #[inline]
    pub const fn len(&self) -> usize {
        N * 32
    }

    #[inline]
    pub const fn as_ptr(&self) -> *const c_void {
        self.inner.as_ptr() as *const c_void
    }

    #[inline]
    pub const fn as_mut_ptr(&mut self) -> *mut c_void {
        self.inner.as_mut_ptr() as *mut c_void
    }

    #[inline]
    pub fn clear_all(&mut self) {
        self.inner.fill(0);
    }

    #[inline]
    pub fn set(&mut self, index: usize) {
        self.inner[index / 32] |= 1 << (index % 32);
    }

    #[inline]
    pub fn reset(&mut self, index: usize) {
        self.inner[index / 32] &= !(1 << (index % 32));
    }

    #[inline]
    pub fn get(&self, index: usize) -> bool {
        self.inner[index / 32] & (1 << (index % 32)) != 0
    }

    /// Count the number of set bits in the array.
    #[inline]
    pub fn count(&self) -> usize {
        self.inner
            .iter()
            .map(|&x| x.count_ones() as usize)
            .sum::<usize>()
    }
}

#[allow(unused)]
impl<const N: usize> AtomicBitArray<N> {
    #[inline]
    pub const fn new() -> Self {
        Self {
            _inner: UnsafeCell::new([0; N]),
        }
    }

    /// # Safety
    ///
    /// `index` must be less than the actual number of elements
    #[inline]
    pub unsafe fn fetch_set_unchecked(&mut self, index: usize) -> bool {
        let result: u8;
        unsafe {
            let p = self.as_mut_ptr();
            asm!(
                "lock bts [{}], {}",
                "setc {}",
                in(reg) p,
                in(reg) index,
                lateout(reg_byte) result,
            );
        }
        result != 0
    }

    /// # Safety
    ///
    /// `index` must be less than the actual number of elements
    #[inline]
    pub unsafe fn fetch_reset_unchecked(&mut self, index: usize) -> bool {
        let result: u8;
        unsafe {
            let p = self.as_mut_ptr();
            asm!(
                "lock btr [{}], {}",
                "setc {}",
                in(reg) p,
                in(reg) index,
                lateout(reg_byte) result,
            );
        }
        result != 0
    }

    /// # Safety
    ///
    /// `index` must be less than the actual number of elements
    #[inline]
    pub unsafe fn fetch_unchecked(&self, index: usize) -> bool {
        let result: u8;
        unsafe {
            let p = self.as_ptr();
            asm!(
                "lock bt [{}], {}",
                "setc {}",
                in(reg) p,
                in(reg) index,
                lateout(reg_byte) result,
            );
        }
        result != 0
    }
}

impl<const N: usize> Deref for AtomicBitArray<N> {
    type Target = BitArray<N>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { transmute(self) }
    }
}

impl<const N: usize> DerefMut for AtomicBitArray<N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { transmute(self) }
    }
}

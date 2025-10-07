use core::{
    arch::asm,
    cell::UnsafeCell,
    mem::transmute,
    ops::{Deref, DerefMut},
};

#[derive(Debug, Clone)]
pub struct BitArray<const N: usize> {
    array: [u32; N],
}

pub struct AtomicBitArray<const N: usize> {
    array: UnsafeCell<[u32; N]>,
}

#[allow(unused)]
impl<const N: usize> BitArray<N> {
    #[inline]
    pub const fn new() -> Self {
        Self { array: [0; N] }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        N * 32
    }

    #[inline]
    pub fn clear_all(&mut self) {
        self.array.fill(0);
    }

    #[inline]
    pub fn set(&mut self, index: usize) {
        self.array[index / 32] |= 1 << (index % 32);
    }

    #[inline]
    pub fn reset(&mut self, index: usize) {
        self.array[index / 32] &= !(1 << (index % 32));
    }

    #[inline]
    pub fn get(&self, index: usize) -> bool {
        self.array[index / 32] & (1 << (index % 32)) != 0
    }

    #[inline]
    pub fn count(&self) -> usize {
        self.array
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
            array: UnsafeCell::new([0; N]),
        }
    }

    /// # Safety
    ///
    /// `index` must be less than the actual number of elements
    #[inline]
    pub unsafe fn fetch_set_unchecked(&mut self, index: usize) -> bool {
        let result: u8;
        unsafe {
            asm!(
                "lock bts [{}], {}",
                "setc {}",
                in(reg) &self.array as *const _ as usize,
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
            asm!(
                "lock btr [{}], {}",
                "setc {}",
                in(reg) &self.array as *const _ as usize,
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
            asm!(
                "lock bt [{}], {}",
                "setc {}",
                in(reg) &self.array as *const _ as usize,
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

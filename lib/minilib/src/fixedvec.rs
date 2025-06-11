use core::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr::{self, copy_nonoverlapping},
    slice,
};

pub struct FixedVec<T, const N: usize> {
    data: [MaybeUninit<T>; N],
    len: usize,
}

impl<T, const N: usize> FixedVec<T, N> {
    #[inline]
    pub const fn new() -> Self {
        Self {
            data: [const { MaybeUninit::uninit() }; N],
            len: 0,
        }
    }

    #[inline]
    pub fn push(&mut self, val: T) -> Result<(), T> {
        if self.len() < self.capacity() {
            unsafe {
                let len = self.len();
                self.data.get_unchecked_mut(len).write(val);
            }
            self.len += 1;
            Ok(())
        } else {
            Err(val)
        }
    }

    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        if self.len() > 0 {
            self.len -= 1;
            unsafe { Some(ptr::read((self.data.as_ptr()).add(self.len())).assume_init()) }
        } else {
            None
        }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// # Panics
    ///
    /// Panics if `new_len` is greater than the current capacity.
    #[inline]
    pub unsafe fn set_len(&mut self, new_len: usize) {
        assert!(new_len <= self.capacity());
        self.len = new_len;
    }

    #[inline]
    pub const fn capacity(&self) -> usize {
        self.data.len()
    }

    #[inline]
    pub fn as_slice<'a>(&'a self) -> &'a [T] {
        let len = self.len();
        unsafe { slice::from_raw_parts(self.as_ptr(), len) }
    }

    #[inline]
    pub const fn as_ptr(&self) -> *const T {
        self.data.as_ptr() as *const T
    }

    #[inline]
    pub fn as_mut_slice<'a>(&'a mut self) -> &'a mut [T] {
        let len = self.len();
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), len) }
    }

    #[inline]
    pub const fn as_mut_ptr(&mut self) -> *mut T {
        self.data.as_mut_ptr() as *mut T
    }

    #[inline]
    pub fn clear(&mut self) {
        let slice: *mut [T] = self.as_mut_slice();
        unsafe {
            self.len = 0;
            ptr::drop_in_place(slice);
        }
    }

    pub fn trancate(&mut self, len: usize) {
        unsafe {
            if len > self.len {
                return;
            }
            let remain = self.len() - len;
            let slice = ptr::slice_from_raw_parts_mut(self.as_mut_ptr().add(len), remain);
            self.len = len;
            ptr::drop_in_place(slice);
        }
    }

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
    {
        self.retain_mut(|elem| f(elem));
    }

    pub fn retain_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        let original_len = self.len;
        self.len = 0;
        let base = self.data.as_mut_ptr();
        for index in 0..original_len {
            let read_ptr = unsafe { base.add(index) };
            let mut temp = unsafe { read_ptr.read() };
            if f(unsafe { temp.assume_init_mut() }) {
                if index != self.len {
                    unsafe {
                        copy_nonoverlapping(read_ptr, base.add(self.len), 1);
                    }
                }
                self.len += 1;
            } else {
                drop(unsafe { read_ptr.read().assume_init() });
            }
        }
    }
}

impl<T, const N: usize> Drop for FixedVec<T, N> {
    fn drop(&mut self) {
        self.clear();
    }
}

impl<T, const N: usize> Deref for FixedVec<T, N> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T, const N: usize> DerefMut for FixedVec<T, N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        let mut vec = FixedVec::<i32, 10>::new();
        assert_eq!(vec.len(), 0);
        assert_eq!(vec.is_empty(), true);
        assert_eq!(vec.capacity(), 10);

        vec.push(12).unwrap();
        assert_eq!(vec.len(), 1);
        assert_eq!(vec.is_empty(), false);

        vec.push(34).unwrap();
        vec.push(56).unwrap();
        vec.push(78).unwrap();
        assert_eq!(vec.len(), 4);
        assert_eq!(vec.is_empty(), false);
        assert_eq!(vec.as_slice(), &[12, 34, 56, 78]);

        vec.push(98).unwrap();
        vec.push(76).unwrap();
        vec.push(54).unwrap();
        vec.push(32).unwrap();
        vec.push(10).unwrap();
        assert_eq!(vec.len(), 9);
        assert_eq!(vec.as_slice(), &[12, 34, 56, 78, 98, 76, 54, 32, 10]);

        vec.pop().unwrap();
        vec.pop().unwrap();
        vec.pop().unwrap();
        vec.pop().unwrap();
        vec.pop().unwrap();
        assert_eq!(vec.len(), 4);
        assert_eq!(vec.as_slice(), &[12, 34, 56, 78,]);

        vec.push(97).unwrap();
        vec.push(86).unwrap();
        vec.push(53).unwrap();
        vec.push(42).unwrap();
        assert_eq!(vec.len(), 8);
        assert_eq!(vec.as_slice(), &[12, 34, 56, 78, 97, 86, 53, 42]);

        vec.clear();
        assert_eq!(vec.len(), 0);
        assert_eq!(vec.is_empty(), true);
        assert_eq!(vec.as_slice(), &[]);
    }

    #[test]
    #[should_panic = "pop_from_zero"]
    fn pop_from_zero() {
        let mut vec = FixedVec::<i32, 10>::new();

        assert_eq!(vec.len(), 0);
        vec.pop().expect("pop_from_zero");
    }

    #[test]
    #[should_panic = "push_out_of_bounds"]
    fn push_out_of_bounds() {
        let mut vec = FixedVec::<i32, 10>::new();

        for i in 0..10 {
            vec.push(i).unwrap();
        }
        assert_eq!(vec.len(), 10);
        vec.push(1234).expect("push_out_of_bounds");
    }

    #[test]
    fn retain() {
        let mut vec = FixedVec::<i32, 10>::new();

        for i in 1..=10 {
            vec.push(i).unwrap();
        }
        assert_eq!(vec.len(), 10);
        assert_eq!(vec.as_slice(), &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

        vec.retain(|elem| (elem % 2) == 0);

        assert_eq!(vec.len(), 5);
        assert_eq!(vec.as_slice(), &[2, 4, 6, 8, 10]);
    }
}

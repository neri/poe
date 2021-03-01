// First In First Out Simple Ring Buffer

use crate::arch::cpu::Cpu;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::*;

pub struct Fifo<T>
where
    T: Sized + Copy,
{
    vec: Vec<UnsafeCell<T>>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl<T> Fifo<T>
where
    T: Sized + Default + Copy,
{
    #[track_caller]
    pub fn new(capacity: usize) -> Self {
        if !capacity.is_power_of_two() {
            panic!(
                "the expected capacity is a power of 2, but the actual capacity is {}",
                capacity
            );
        }
        let mut vec = Vec::with_capacity(capacity);
        vec.resize_with(capacity, || UnsafeCell::new(T::default()));

        Self {
            vec,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    #[inline]
    pub fn mask(&self) -> usize {
        self.vec.len() - 1
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::SeqCst) == self.tail.load(Ordering::SeqCst)
    }

    pub unsafe fn enqueue(&self, data: T) -> Result<(), T> {
        let old_tail = self.tail.load(Ordering::SeqCst);
        let new_tail = (old_tail + 1) & self.mask();
        if new_tail == self.head.load(Ordering::SeqCst) {
            Err(data)
        } else {
            let p = self.vec.get_unchecked(old_tail).get();
            p.write_volatile(data);
            self.tail.store(new_tail, Ordering::SeqCst);
            Ok(())
        }
    }

    pub unsafe fn dequeue(&self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            let head = self.head.load(Ordering::SeqCst);
            let p = self.vec.get_unchecked(head).get();
            let r = p.read_volatile();
            self.head.store((head + 1) & self.mask(), Ordering::SeqCst);
            Some(r)
        }
    }
}

pub struct InterlockedFifo<T>
where
    T: Sized + Copy,
{
    wrapped: Fifo<T>,
}

impl<T> InterlockedFifo<T>
where
    T: Sized + Default + Copy,
{
    #[track_caller]
    pub fn new(capacity: usize) -> Self {
        Self {
            wrapped: Fifo::new(capacity),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.wrapped.is_empty()
    }

    pub fn enqueue(&self, data: T) -> Result<(), T> {
        unsafe { Cpu::without_interrupts(|| self.wrapped.enqueue(data)) }
    }

    pub fn dequeue(&self) -> Option<T> {
        unsafe { Cpu::without_interrupts(|| self.wrapped.dequeue()) }
    }
}

unsafe impl<T: Copy> Send for InterlockedFifo<T> {}

unsafe impl<T: Copy> Sync for InterlockedFifo<T> {}

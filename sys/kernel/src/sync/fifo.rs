// First In First Out Simple Ring Buffer

use crate::arch::cpu::Cpu;
use alloc::vec::Vec;

pub struct Fifo<T>
where
    T: Sized + Copy,
{
    vec: Vec<T>,
    head: usize,
    tail: usize,
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
        vec.resize(capacity, T::default());

        Self {
            vec,
            head: 0,
            tail: 0,
        }
    }

    #[inline]
    pub fn mask(&self) -> usize {
        self.vec.len() - 1
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    pub fn enqueue(&mut self, data: T) -> Result<(), T> {
        let old_tail = self.tail;
        let new_tail = (old_tail + 1) & self.mask();
        if new_tail == self.head {
            Err(data)
        } else {
            unsafe {
                *self.vec.get_unchecked_mut(old_tail) = data;
                self.tail = new_tail;
                Ok(())
            }
        }
    }

    pub fn dequeue(&mut self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            unsafe {
                let r = self.vec.get_unchecked(self.head);
                self.head = (self.head + 1) & self.mask();
                Some(*r)
            }
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
    pub const fn is_empty(&self) -> bool {
        self.wrapped.is_empty()
    }

    pub fn enqueue(&mut self, data: T) -> Result<(), T> {
        unsafe { Cpu::without_interrupts(|| self.wrapped.enqueue(data)) }
    }

    pub fn dequeue(&mut self) -> Option<T> {
        unsafe { Cpu::without_interrupts(|| self.wrapped.dequeue()) }
    }
}

//! View for control registers and flags.

use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

#[must_use = "Did you forget to call `enable()` or `disable()`?"]
pub struct ControlRegisterView<'a, T> {
    value: &'a mut T,
    bit: T,
}

impl<'a, T> ControlRegisterView<'a, T>
where
    T: Copy
        + BitAnd<Output = T>
        + BitAndAssign<T>
        + BitOr<Output = T>
        + BitOrAssign<T>
        + Not<Output = T>
        + PartialEq,
{
    #[inline]
    pub fn new(value: &'a mut T, bit: T) -> Self {
        Self { value, bit }
    }

    #[inline]
    pub fn enable(&mut self) {
        *self.value |= self.bit;
    }

    #[inline]
    pub fn disable(&mut self) {
        *self.value &= !self.bit;
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        (*self.value & self.bit) == self.bit
    }

    #[inline]
    pub fn is_disabled(&self) -> bool {
        !self.is_enabled()
    }

    #[inline]
    pub fn set(&mut self, value: bool) {
        if value {
            self.enable();
        } else {
            self.disable();
        }
    }
}

pub struct ControlRegisterReadonlyView<'a, T> {
    value: &'a T,
    bit: T,
}

impl<'a, T> ControlRegisterReadonlyView<'a, T>
where
    T: Copy + BitAnd<Output = T> + BitOr<Output = T> + Not<Output = T> + PartialEq,
{
    #[inline]
    pub fn new(value: &'a T, bit: T) -> Self {
        Self { value, bit }
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        (*self.value & self.bit) == self.bit
    }

    #[inline]
    pub fn is_disabled(&self) -> bool {
        !self.is_enabled()
    }
}

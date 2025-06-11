//! Hardware Abstraction Layer

use super::InterruptGuard;
use core::{
    ffi::c_void,
    fmt,
    ops::{Add, BitAnd, BitOr, Mul, Not, Sub},
    sync::atomic::{Ordering, compiler_fence},
};

#[allow(unused_imports)]
use core::num::{NonZeroU32, NonZeroU64};

// impl !Send for InterruptGuard {}

// impl !Sync for InterruptGuard {}

pub struct Hal;

#[allow(unused)]
pub trait HalTrait {
    fn cpu() -> impl HalCpu;
}

#[allow(unused)]
pub trait HalCpu {
    fn no_op(&self);

    fn wait_for_interrupt(&self);

    unsafe fn enable_interrupt(&self);

    unsafe fn disable_interrupt(&self);

    unsafe fn is_interrupt_enabled(&self) -> bool;

    #[inline]
    unsafe fn is_interrupt_disabled(&self) -> bool {
        unsafe { !self.is_interrupt_enabled() }
    }

    #[inline]
    unsafe fn set_interrupt_enabled(&self, enabled: bool) {
        if enabled {
            unsafe {
                self.enable_interrupt();
            }
        } else {
            unsafe {
                self.disable_interrupt();
            }
        }
    }

    #[must_use]
    unsafe fn interrupt_guard(&self) -> InterruptGuard;

    #[inline]
    fn stop(&self) -> ! {
        compiler_fence(Ordering::SeqCst);
        loop {
            unsafe {
                self.disable_interrupt();
                self.wait_for_interrupt();
            }
        }
    }
}

#[macro_export]
macro_rules! without_interrupts {
    ( $f:expr ) => {{
        let flags = unsafe { Hal::cpu().interrupt_guard() };
        let result = { $f };
        drop(flags);
        result
    }};
}

#[cfg(target_pointer_width = "32")]
pub type PhysicalAddressRepr = u32;
#[cfg(target_pointer_width = "32")]
pub type NonZeroPhysicalAddressRepr = NonZeroU32;
#[cfg(target_pointer_width = "64")]
pub type PhysicalAddressRepr = u64;
#[cfg(target_pointer_width = "64")]
pub type NonZeroPhysicalAddressRepr = NonZeroU64;

#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysicalAddress(PhysicalAddressRepr);

impl PhysicalAddress {
    pub const NULL: Self = Self(0);

    #[inline]
    pub const fn new(val: PhysicalAddressRepr) -> Self {
        Self(val as PhysicalAddressRepr)
    }

    #[inline]
    pub const fn from_usize(val: usize) -> Self {
        Self(val as PhysicalAddressRepr)
    }

    #[inline]
    pub fn from_ptr(val: *const c_void) -> Self {
        Self(val as usize as PhysicalAddressRepr)
    }

    #[cfg(target_pointer_width = "32")]
    #[inline]
    pub const fn from_u32(val: u32) -> Self {
        Self(val as PhysicalAddressRepr)
    }

    #[inline]
    pub const fn from_u64(val: u64) -> Self {
        Self(val as PhysicalAddressRepr)
    }

    #[inline]
    pub const fn as_repr(&self) -> PhysicalAddressRepr {
        self.0 as PhysicalAddressRepr
    }

    #[cfg(target_pointer_width = "32")]
    #[inline]
    pub const fn as_u32(&self) -> u32 {
        self.0 as u32
    }

    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0 as u64
    }

    #[inline]
    pub const fn as_usize(&self) -> usize {
        self.0 as usize
    }

    #[inline]
    pub fn rounding_up(&self, align: PhysicalAddressRepr) -> Self {
        let mask = align - 1;
        Self((self.0 + mask) & !(mask))
    }
}

impl Default for PhysicalAddress {
    #[inline]
    fn default() -> Self {
        Self(Default::default())
    }
}

impl Add<usize> for PhysicalAddress {
    type Output = Self;

    #[inline]
    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs as PhysicalAddressRepr)
    }
}

impl Add<PhysicalAddressRepr> for PhysicalAddress {
    type Output = Self;

    #[inline]
    fn add(self, rhs: PhysicalAddressRepr) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Sub<PhysicalAddress> for PhysicalAddress {
    type Output = usize;

    #[inline]
    fn sub(self, rhs: PhysicalAddress) -> Self::Output {
        (self.0 - rhs.0) as usize
    }
}

impl Sub<usize> for PhysicalAddress {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: usize) -> Self::Output {
        Self(self.0 - rhs as PhysicalAddressRepr)
    }
}

impl Mul<usize> for PhysicalAddress {
    type Output = Self;

    fn mul(self, rhs: usize) -> Self::Output {
        Self(self.0 * rhs as PhysicalAddressRepr)
    }
}

impl Mul<PhysicalAddressRepr> for PhysicalAddress {
    type Output = Self;

    fn mul(self, rhs: PhysicalAddressRepr) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl BitAnd<PhysicalAddressRepr> for PhysicalAddress {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: PhysicalAddressRepr) -> Self::Output {
        Self(self.0 & rhs)
    }
}

impl BitAnd<PhysicalAddress> for PhysicalAddressRepr {
    type Output = Self;

    fn bitand(self, rhs: PhysicalAddress) -> Self::Output {
        self & rhs.0
    }
}

impl BitOr<PhysicalAddressRepr> for PhysicalAddress {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: PhysicalAddressRepr) -> Self::Output {
        Self(self.0 | rhs)
    }
}

impl Not for PhysicalAddress {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

impl From<PhysicalAddressRepr> for PhysicalAddress {
    #[inline]
    fn from(val: PhysicalAddressRepr) -> Self {
        Self::new(val)
    }
}

impl From<PhysicalAddress> for PhysicalAddressRepr {
    #[inline]
    fn from(val: PhysicalAddress) -> Self {
        val.as_repr()
    }
}

impl fmt::LowerHex for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> core::fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

// impl core::iter::Step for PhysicalAddress {
//     #[inline]
//     fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
//         PhysicalAddressRepr::steps_between(&start.0, &end.0)
//     }

//     #[inline]
//     fn forward_checked(start: Self, count: usize) -> Option<Self> {
//         PhysicalAddressRepr::forward_checked(start.0, count).map(|v| PhysicalAddress(v))
//     }

//     #[inline]
//     fn backward_checked(start: Self, count: usize) -> Option<Self> {
//         PhysicalAddressRepr::backward_checked(start.0, count).map(|v| PhysicalAddress(v))
//     }
// }

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NonNullPhysicalAddress(NonZeroPhysicalAddressRepr);

impl NonNullPhysicalAddress {
    #[inline]
    pub const fn get(&self) -> PhysicalAddress {
        PhysicalAddress(self.0.get())
    }

    #[inline]
    pub const fn new(val: PhysicalAddress) -> Option<Self> {
        match NonZeroPhysicalAddressRepr::new(val.as_repr()) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    #[inline]
    pub const fn from_usize(val: usize) -> Option<Self> {
        Self::new(PhysicalAddress(val as PhysicalAddressRepr))
    }

    #[inline]
    pub fn from_ptr(val: *const c_void) -> Option<Self> {
        Self::new(PhysicalAddress(val as usize as PhysicalAddressRepr))
    }

    #[inline]
    pub const unsafe fn new_unchecked(val: PhysicalAddress) -> Self {
        unsafe { Self(NonZeroPhysicalAddressRepr::new_unchecked(val.as_repr())) }
    }
}

impl From<NonNullPhysicalAddress> for PhysicalAddress {
    #[inline]
    fn from(val: NonNullPhysicalAddress) -> Self {
        val.get()
    }
}

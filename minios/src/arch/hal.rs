//! Hardware Abstraction Layer

use alloc::rc::Rc;
use core::{
    ffi::c_void,
    fmt,
    marker::PhantomData,
    ops::{Add, BitAnd, BitOr, Mul, Not, Sub},
    sync::atomic::{Ordering, compiler_fence},
};

#[allow(unused_imports)]
use core::num::{NonZeroU32, NonZeroU64};

pub struct Hal;

#[allow(unused)]
pub trait HalTrait {
    /// Returns an implementation of the `HalCpu` trait, which provides CPU-specific functionality such as interrupt management and special instructions.
    fn cpu() -> impl HalCpu;
}

#[allow(unused)]
pub trait HalCpu {
    /// Executes a `no-op` instruction.
    fn no_op(&self);

    /// Executes a `wait-for-interrupt` instruction, putting the CPU into a low-power state until an interrupt occurs.
    ///
    /// NOTE: This function does not guarantee that the CPU will actually enter a low-power state
    fn wait_for_interrupt(&self);

    /// Executes an invalid instruction, causing the CPU to raise an exception.
    fn bad_instruction(&self) -> !;

    /// Enables interrupts on the CPU, allowing it to respond to external events.
    unsafe fn enable_interrupt(&self);

    /// Disables interrupts on the CPU, preventing it from responding to external events.
    unsafe fn disable_interrupt(&self);

    /// Checks if interrupts are currently enabled on the CPU.
    fn is_interrupt_enabled(&self) -> bool;

    /// Checks if interrupts are currently disabled on the CPU.
    #[inline]
    fn is_interrupt_disabled(&self) -> bool {
        unsafe { !self.is_interrupt_enabled() }
    }

    /// Sets the interrupt enabled state of the CPU.
    #[inline]
    unsafe fn set_interrupt_enabled(&self, enabled: bool) {
        unsafe {
            if enabled {
                self.enable_interrupt();
            } else {
                self.disable_interrupt();
            }
        }
    }

    /// Creates an interrupt guard that disables interrupts when created and re-enables them when dropped.
    /// This is useful for ensuring that interrupts are properly re-enabled after a critical section of code.
    #[must_use]
    unsafe fn interrupt_guard(&self) -> InterruptGuard;

    /// Halts the CPU indefinitely.
    /// This is typically used in situations where the system cannot continue running, such as after a fatal error.
    #[inline]
    fn halt(&self) -> ! {
        compiler_fence(Ordering::SeqCst);
        loop {
            unsafe {
                self.disable_interrupt();
                self.wait_for_interrupt();
            }
        }
    }

    /// Atomically loads a 64-bit counter value from the given pointer.
    #[inline]
    fn atomic_u64_load(&self, p: &u64) -> u64 {
        if cfg!(target_pointer_width = "32") {
            unsafe {
                let p = p as *const u64 as *const u32;
                let mut hi = p.add(1).read_volatile();
                let mut lo = p.read_volatile();
                loop {
                    let hi2 = p.add(1).read_volatile();
                    if hi == hi2 {
                        return (hi as u64) << 32 | (lo as u64);
                    }
                    hi = hi2;
                    lo = p.read_volatile();
                }
            }
        } else {
            unsafe { core::ptr::read_volatile(p) }
        }
    }
}

/// Executes a closure with interrupts disabled, ensuring that interrupts are properly re-enabled after the closure is executed.
#[macro_export]
macro_rules! without_interrupts {
    ( $f:expr ) => {{
        let guard = Hal::cpu().interrupt_guard();
        let result = { $f };
        drop(guard);
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

    /// Aligns the address up to the nearest multiple of `align`. `align` must be a power of two.
    #[inline]
    pub fn rounding_up(&self, align: PhysicalAddressRepr) -> Self {
        let mask = align - 1;
        Self((self.0 + mask) & !(mask))
    }

    /// Aligns the address up to the nearest multiple of 4 KiB.
    ///
    /// NOTE: 4 KiB is the common page size on many platforms.
    #[inline]
    pub fn rounding_up_4k(&self) -> Self {
        self.rounding_up(0x1000)
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

#[must_use = "InterruptGuard will re-enable interrupts when dropped, so it must be used to ensure interrupts are properly re-enabled."]
pub struct InterruptGuard {
    flags: usize,

    // To prevent `Send` and `Sync` auto traits
    _phantom: PhantomData<Rc<()>>,
}

impl Drop for InterruptGuard {
    #[inline]
    fn drop(&mut self) {
        compiler_fence(Ordering::SeqCst);
        if self.flags != 0 {
            unsafe {
                Hal::cpu().enable_interrupt();
            }
        }
    }
}

impl InterruptGuard {
    #[inline]
    pub(super) const unsafe fn new(flags: usize) -> Self {
        Self {
            flags,
            _phantom: PhantomData,
        }
    }
}

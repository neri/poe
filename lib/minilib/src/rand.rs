//! Random Number Generator

use core::{
    convert::Infallible,
    num::{NonZeroU32, NonZeroU64},
};

/// Random Number Generator
pub trait Rng {
    type Output;
    type Error;

    fn rand(&mut self) -> Result<Self::Output, Self::Error>;
}

/// Pseudo Random Number Generator
pub trait Prng {
    type Output;

    fn next(&mut self) -> Self::Output;
}

impl<T: Prng> Rng for T {
    type Output = <T as Prng>::Output;
    type Error = Infallible;

    #[inline]
    fn rand(&mut self) -> Result<Self::Output, Infallible> {
        Ok(self.next())
    }
}

pub struct XorShift64 {
    seed: u64,
}

impl XorShift64 {
    #[inline]
    pub const fn new(seed: NonZeroU64) -> Self {
        Self { seed: seed.get() }
    }
}

impl Default for XorShift64 {
    #[inline]
    fn default() -> Self {
        Self {
            seed: 88172645463325252,
        }
    }
}

impl Prng for XorShift64 {
    type Output = u64;

    #[inline]
    fn next(&mut self) -> u64 {
        let mut x = self.seed;
        x = x ^ (x << 7);
        x = x ^ (x >> 9);
        self.seed = x;
        x
    }
}

pub struct XorShift32 {
    seed: u32,
}

impl XorShift32 {
    #[inline]
    pub const fn new(seed: NonZeroU32) -> Self {
        Self { seed: seed.get() }
    }
}

impl Default for XorShift32 {
    #[inline]
    fn default() -> Self {
        Self { seed: 2463534242 }
    }
}

impl Prng for XorShift32 {
    type Output = u32;

    #[inline]
    fn next(&mut self) -> u32 {
        let mut x = self.seed;
        x = x ^ (x << 13);
        x = x ^ (x >> 17);
        x = x ^ (x << 5);
        self.seed = x;
        x
    }
}

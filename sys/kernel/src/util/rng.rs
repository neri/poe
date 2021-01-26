// Random Number Generator

pub trait Rng {
    type Output;
    fn rand(&mut self) -> Result<Self::Output, ()>;
}

pub struct XorShift64 {
    seed: u64,
}

impl XorShift64 {
    pub const fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.seed;
        x = x ^ (x << 7);
        x = x ^ (x >> 9);
        self.seed = x;
        x
    }
}

impl Default for XorShift64 {
    fn default() -> Self {
        Self::new(88172645463325252)
    }
}

impl Rng for XorShift64 {
    type Output = u64;
    fn rand(&mut self) -> Result<Self::Output, ()> {
        Ok(self.next())
    }
}

pub struct XorShift32 {
    seed: u32,
}

impl XorShift32 {
    pub const fn new(seed: u32) -> Self {
        Self { seed }
    }

    pub fn next(&mut self) -> u32 {
        let mut x = self.seed;
        x = x ^ (x << 13);
        x = x ^ (x >> 17);
        x = x ^ (x << 5);
        self.seed = x;
        x
    }
}

impl Default for XorShift32 {
    fn default() -> Self {
        Self::new(2463534242)
    }
}

impl Rng for XorShift32 {
    type Output = u32;
    fn rand(&mut self) -> Result<Self::Output, ()> {
        Ok(self.next())
    }
}

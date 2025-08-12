use crate::prot::{Linear32, Selector};

/// Type of 16bit Offset Address
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Offset16(u16);

impl Offset16 {
    #[inline]
    pub const fn new(off: u16) -> Self {
        Self(off)
    }

    #[inline]
    pub const fn as_u16(&self) -> u16 {
        self.0
    }

    #[inline]
    pub const fn as_u32(&self) -> u32 {
        self.0 as u32
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Far16Ptr(u32);

impl Far16Ptr {
    pub const NULL: Far16Ptr = Far16Ptr(0);

    #[inline]
    pub const fn new(seg: Selector, off: Offset16) -> Self {
        Self((seg.as_u16() as u32) * 0x10000 + off.0 as u32)
    }

    #[inline]
    pub const fn from_u32(val: u32) -> Self {
        Self(val)
    }

    #[inline]
    pub const fn as_u32(&self) -> u32 {
        self.0
    }

    #[inline]
    pub const fn off(&self) -> Offset16 {
        Offset16((self.0 & 0xffff) as u16)
    }

    #[inline]
    pub const fn sel(&self) -> Selector {
        Selector((self.0 >> 16) as u16)
    }

    /// Create a Far16 from a linear address.
    ///
    /// TODO: Support for addresses larger than 1 MB
    #[inline]
    pub const fn from_linear(linear: Linear32) -> Self {
        let linear = linear.as_u32();
        let off = (linear & 0x000f) as u16;
        let seg = (linear >> 4) as u16;
        Self::new(Selector(seg), Offset16(off))
    }

    #[inline]
    pub const fn to_linear(&self) -> Linear32 {
        Linear32::new(self.sel().as_u16() as u32 * 16 + self.off().0 as u32)
    }

    #[inline]
    pub fn to_ptr<T>(&self) -> *mut T {
        self.to_linear().as_u32() as *mut T
    }
}

impl From<Far16Ptr> for Linear32 {
    #[inline]
    fn from(far: Far16Ptr) -> Self {
        far.to_linear()
    }
}

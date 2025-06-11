//! Compact & Efficient Executable Format (unstable)
//!
//! Although the ELF format is general-purpose, it is not suitable for storing kernel data due to unnecessary margins.
//!
//! This format is specialized for kernel storage and is not suitable for general application interchange use
//!

use core::mem::{size_of, transmute};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum CeefVersion {
    /// Version 0, segmented, not compressed
    #[default]
    V0 = 0,
    /// Version 1, not segmented, compressed
    V1 = 1,
}

impl CeefVersion {
    pub const CURRENT: Self = Self::V1;
}

impl core::fmt::Display for CeefVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", *self as u8)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CeefHeader {
    pub magic: u16,
    pub version: CeefVersion,
    pub n_secs: u8,
    pub entry: u32,
    pub base: u32,
    pub minalloc: u32,
}

impl CeefHeader {
    pub const MAGIC: u16 = 0xCEEF;

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC && self.version <= CeefVersion::CURRENT
    }

    #[inline]
    pub fn as_bytes(self) -> [u8; 16] {
        unsafe { transmute(self) }
    }

    #[inline]
    pub const fn n_secs(&self) -> usize {
        self.n_secs as usize
    }

    #[inline]
    pub fn size_of_headers(&self) -> usize {
        size_of::<Self>() + self.n_secs() * size_of::<CeefSecHeader>()
    }
}

impl Default for CeefHeader {
    fn default() -> Self {
        Self {
            magic: Self::MAGIC,
            version: Default::default(),
            n_secs: 0,
            entry: 0,
            base: 0,
            minalloc: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CeefSecHeader {
    pub attr: u8,
    _reserved: [u8; 3],
    pub filesz: u32,
    pub vaddr: u32,
    pub memsz: u32,
}

impl CeefSecHeader {
    pub const fn new(attr: u8, vaddr: u32, filesz: u32, memsz: u32, align: u8) -> Self {
        Self {
            attr: (attr << 5) | (align & 31),
            _reserved: [0, 0, 0],
            vaddr,
            filesz,
            memsz,
        }
    }

    pub fn as_bytes(self) -> [u8; 16] {
        unsafe { transmute(self) }
    }

    pub const fn attr(&self) -> usize {
        (self.attr >> 5) as usize
    }

    pub const fn align(&self) -> usize {
        (self.attr & 31) as usize
    }
}

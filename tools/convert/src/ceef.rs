// Compact & Efficient Executable Format

use core::mem::transmute;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CeefHeader {
    pub magic: u16,
    pub version: u8,
    pub n_sec: u8,
    _reserved: u32,
    pub entry: u32,
    pub imagesz: u32,
}

impl CeefHeader {
    pub const MAGIC: u16 = 0xCEEF;
    pub const VER_CURRENT: u8 = 1;

    pub const fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC && self.version == Self::VER_CURRENT
    }

    pub fn as_bytes(self) -> [u8; 16] {
        unsafe { transmute(self) }
    }
}

impl Default for CeefHeader {
    fn default() -> Self {
        Self {
            magic: Self::MAGIC,
            version: Self::VER_CURRENT,
            n_sec: 0,
            _reserved: 0,
            entry: 0,
            imagesz: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CeefSecHeader {
    pub attr: u8,
    _reserved1: [u8; 3],
    _reserved2: u32,
    pub rva: u32,
    pub size: u32,
}

impl CeefSecHeader {
    pub const fn new(attr: u8, rva: u32, size: u32) -> Self {
        Self {
            attr,
            _reserved1: [0, 0, 0],
            _reserved2: 0,
            rva,
            size,
        }
    }

    pub fn as_bytes(self) -> [u8; 16] {
        unsafe { transmute(self) }
    }
}

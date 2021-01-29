// TOE Boot Protocol
#![no_std]

use core::fmt;

#[repr(C)]
pub struct BootInfo {
    pub platform: Platform,
    pub bios_boot_drive: u8,
    _boot_flags: u16,
    pub vram_base: u32,
    pub screen_width: u16,
    pub screen_height: u16,
    pub screen_stride: u16,
    pub screen_bpp: u8,
    _reserved_1: u8,
    pub kernel_end: u32,
    pub acpi: u32,
    pub smap: (u32, u32),
}

#[non_exhaustive]
#[repr(i8)]
#[derive(Debug, Clone, Copy)]
pub enum Platform {
    Unknown = -1,
    Nec98 = 0,
    PcCompatible = 1,
    FmTowns = 2,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PcCompatible => write!(f, "PC Compatible"),
            Self::Nec98 => write!(f, "PC-98"),
            Self::FmTowns => write!(f, "FM TOWNS"),
            _ => write!(f, "Unknown"),
        }
    }
}

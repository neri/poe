// TOE Boot Protocol
#![no_std]

use core::fmt;

#[repr(C)]
pub struct BootInfo {
    pub platform: Platform,
    pub bios_boot_drive: u8,

    /// CPU Version or Generation
    pub cpu_ver: CpuVersion,

    /// Screen bit per pixel
    /// 0 or 8 means 8bpp, 32 means 32bpp
    pub screen_bpp: u8,

    /// Screen informations
    pub vram_base: u32,
    pub screen_width: u16,
    pub screen_height: u16,
    pub screen_stride: u16,

    /// TBD
    _boot_flags: u16,

    /// TODO: ACPI
    pub acpi: u32,

    /// TODO: initrd
    pub initrd_base: u32,
    pub initrd_size: u32,

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

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CpuVersion(pub u8);

impl CpuVersion {
    /// Unspecified CPU Type (default)
    pub const UNSPECIFIED: Self = Self(0);
    /// The CPU is 386 level
    pub const X86_386: Self = Self(3);
    /// The CPU is 486 level
    pub const X86_486: Self = Self(4);
    /// The CPU has a CPUID instruction, which does not mean 586 or Pentium.
    pub const X86_HAS_CPUID: Self = Self(5);
    /// The CPU supports the AMD64 instruction set. (reserved)
    pub const X86_AMD64: Self = Self(6);
}

impl Default for CpuVersion {
    fn default() -> Self {
        Self::UNSPECIFIED
    }
}

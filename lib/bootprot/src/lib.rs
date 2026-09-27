//! MEG-OS Boot Procotol

#![no_std]

use core::fmt;

#[repr(C)]
pub struct BootInfo {
    pub platform: PlatformType,
    pub color_mode: ColorMode,
    pub screen_width: u16,
    pub screen_height: u16,
    pub vram_stride: u16,
    pub vram_base: u64,

    pub master_page_table: u64,
    pub acpi_rsdptr: u64,
    pub dtb: u64,
    pub smbios: u64,
    pub kernel_base: u64,
    pub total_memory_size: u64,
    pub cmdline: u64,
    pub initrd_base: u32,
    pub initrd_size: u32,
    pub mmap_base: u32,
    pub mmap_len: u32,
    pub real_bitmap: [u32; 8],
    pub flags: BootFlags,
}

#[repr(u8)]
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformType {
    // #[default]
    // Unspecified = 0,
    /// IA32-Legacy NEC PC-98 Series Computer
    Nec98 = 1,
    /// IA32-Legacy IBM PC Compatible
    PcBios = 2,
    /// IA32-Legacy Fujitsu FM TOWNS
    FmTowns = 3,
    /// Native UEFI based platform
    UefiNative = 4,
    /// Non native UEFI based platform
    UefiBridged = 5,
    /// Device Tree based platforms
    DeviceTree = 6,
    /// Raspberry Pi
    RaspberryPi = 7,
    /// RISC-V with SBI
    Sbi = 8,
}

impl PlatformType {
    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PcBios => "PC (BIOS)",
            Self::Nec98 => "PC-98",
            Self::FmTowns => "FM TOWNS",
            Self::UefiNative => "UEFI",
            Self::UefiBridged => "UEFI (Bridged)",
            Self::DeviceTree => "Device Tree",
            Self::RaspberryPi => "Raspberry Pi",
            Self::Sbi => "RISC-V with SBI",
        }
    }
}

impl core::fmt::Display for PlatformType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[non_exhaustive]
#[repr(u8)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorMode {
    #[default]
    Unspecified = 0,
    /// 8bit Indexed Color Mode
    Indexed8 = 8,
    /// 32bit Color (Little Endian B-G-R-A, VESA, UEFI)
    Argb32 = 32,
    // 32bit Color (Big Endian R-G-B-A)
    Abgr32 = 33,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct BootFlags(u32);

impl BootFlags {
    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }
}

impl Default for BootFlags {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootMemoryMapDescriptor {
    pub base: u64,
    pub page_count: u32,
    pub mem_type: BootMemoryType,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BootMemoryType {
    Available = 0,
    AcpiReclaim = 1,
    AcpiNonVolatile = 2,
    Mmio = 3,
    MmioPortSpace = 4,
    OsLoaderCode = 5,
    /// Used by the loader until it passes control; reclaimable after that
    OsLoaderData = 6,
    FirmwareCode = 7,
    FirmwareData = 8,
    Reserved = 9,
    Unavailable = 10,
    /// The kernel image
    Kernel = 11,
    /// Boot information: this memory map, the command line, ...
    BootData = 12,
    /// The initial ramdisk
    Initrd = 13,
    /// The device tree blob received from the firmware, unchanged
    DeviceTree = 14,
    /// Memory that keeps the display on; keep it until the display is taken over
    Framebuffer = 15,
}

/// One entry of the memory map, with 64-bit base and size
///
/// Unlike [`BootMemoryMapDescriptor`], the size is in bytes and never
/// truncated, and the origin of the region is kept in `attributes`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootMemoryRegion {
    pub base: u64,
    pub size: u64,
    pub mem_type: BootMemoryType,
    pub attributes: BootMemoryAttributes,
}

/// Attributes of a [`BootMemoryRegion`]
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BootMemoryAttributes(pub u32);

impl BootMemoryAttributes {
    /// Backed by system RAM
    pub const RAM: Self = Self(1 << 0);
    /// Device tree `no-map`: not to be mapped as normal memory
    pub const NO_MAP: Self = Self(1 << 1);
    /// Device tree `reusable`
    pub const REUSABLE: Self = Self(1 << 2);
    /// From the memory reservation block of the DTB
    pub const MEMRESERVE: Self = Self(1 << 8);
    /// From `/reserved-memory` of the DTB
    pub const RESERVED_MEMORY: Self = Self(1 << 9);
    /// From a firmware memory map or a platform specific source
    pub const FIRMWARE: Self = Self(1 << 10);

    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

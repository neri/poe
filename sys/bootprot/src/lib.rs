// TOE Boot Protocol
#![no_std]

#[repr(C)]
pub struct BootInfo {
    pub platform: BootPlatform,
    pub bios_boot_drive: u8,
    _reserved_1: [u8; 2],
    pub memsz_lo: u16,
    pub memsz_mi: u16,
    pub memsz_hi: u32,
    pub kernel_end: u32,
    pub vram_base: u32,
    pub screen_width: u16,
    pub screen_height: u16,
    pub screen_stride: u16,
    pub screen_bpp: u8,
    _reserved_2: u8,
    pub acpi: u32,
}

#[non_exhaustive]
#[repr(i8)]
#[derive(Debug, Clone, Copy)]
pub enum BootPlatform {
    Unknown = -1,
    Nec98 = 0,
    PcCompatible = 1,
    FmTowns = 2,
}

impl BootPlatform {
    pub const fn name(&self) -> &str {
        match self {
            Self::PcCompatible => "PC Compatible",
            Self::Nec98 => "PC-98",
            Self::FmTowns => "FM TOWNS",
            _ => "Unknown",
        }
    }
}

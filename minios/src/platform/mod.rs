//! Platform-specific code.

#[cfg(feature = "pc")]
pub mod x86_pc;
#[cfg(feature = "pc")]
pub use x86_pc as current;

#[cfg(feature = "rpi")]
pub mod rpi;
#[cfg(feature = "rpi")]
pub use rpi as current;

#[cfg(feature = "sbi")]
pub mod rv_sbi;
#[cfg(feature = "sbi")]
pub use rv_sbi as current;

use core::fmt;

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum Platform {
    Unknown = 0,
    Nec98 = 1,
    PcBios = 2,
    FmTowns = 3,
    Uefi = 4,
    DeviceTree = 5,
    RaspberryPi = 6,
    OpenSbi = 7,
}

impl Platform {
    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::PcBios => "PC (BIOS)",
            Self::Nec98 => "PC-98",
            Self::FmTowns => "FM TOWNS",
            Self::Uefi => "UEFI",
            Self::DeviceTree => "Device Tree",
            Self::RaspberryPi => "Raspberry Pi",
            Self::OpenSbi => "OpenSBI",
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub trait PlatformTrait {
    #[cfg(feature = "device_tree")]
    unsafe fn init_dt_early(dt: &fdt::DeviceTree, arg: usize);

    unsafe fn init(arg: usize);

    unsafe fn exit();

    fn reset_system() -> !;
}

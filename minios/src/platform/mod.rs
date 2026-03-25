//! Platform-specific code.

#[cfg(feature = "pc")]
pub mod x86_pc;
#[cfg(feature = "pc")]
pub use x86_pc as current;

#[cfg(feature = "rpi")]
pub mod rpi;
#[cfg(feature = "rpi")]
pub use rpi as current;

#[cfg(all(
    any(target_arch = "riscv32", target_arch = "riscv64"),
    feature = "virt"
))]
pub mod rv_virt;
#[cfg(all(
    any(target_arch = "riscv32", target_arch = "riscv64"),
    feature = "virt"
))]
pub use rv_virt as current;

use core::fmt;

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum Platform {
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
    /// Virt Machine (e.g. QEMU virt)
    Virt = 8,
}

impl Platform {
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
            Self::Virt => "Virt Machine",
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

pub trait PlatformTimer {
    // todo
}

// A Computer System

use crate::graphics::color::*;
use crate::graphics::coords::*;
use crate::graphics::emcon::*;
use crate::*;
use crate::{fonts::FontManager, graphics::bitmap::*};
use arch::cpu::Cpu;
use core::fmt;
use toeboot::*;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    versions: u32,
    rel: &'static str,
}

impl Version {
    const SYSTEM_NAME: &'static str = "codename TOE";
    const SYSTEM_SHORT_NAME: &'static str = "TOE";
    const RELEASE: &'static str = "";
    const VERSION: Version = Version::new(0, 0, 1, Self::RELEASE);

    #[inline]
    const fn new(maj: u8, min: u8, patch: u16, rel: &'static str) -> Self {
        let versions = ((maj as u32) << 24) | ((min as u32) << 16) | (patch as u32);
        Version { versions, rel }
    }

    #[inline]
    pub const fn as_u32(&self) -> u32 {
        self.versions
    }

    #[inline]
    pub const fn maj(&self) -> usize {
        ((self.versions >> 24) & 0xFF) as usize
    }

    #[inline]
    pub const fn min(&self) -> usize {
        ((self.versions >> 16) & 0xFF) as usize
    }

    #[inline]
    pub const fn patch(&self) -> usize {
        (self.versions & 0xFFFF) as usize
    }

    #[inline]
    pub const fn release(&self) -> &str {
        self.rel
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.rel.len() > 0 {
            write!(
                f,
                "{}.{}.{}-{}",
                self.maj(),
                self.min(),
                self.patch(),
                self.rel
            )
        } else {
            write!(f, "{}.{}.{}", self.maj(), self.min(), self.patch())
        }
    }
}

pub struct System {
    main_screen: Option<Bitmap<'static>>,
    em_console: EmConsole,
    platform: Platform,
    cpu_ver: CpuVersion,
}

static mut SYSTEM: System = System::new();

impl System {
    const fn new() -> Self {
        Self {
            main_screen: None,
            em_console: EmConsole::new(),
            platform: Platform::Unknown,
            cpu_ver: CpuVersion::UNSPECIFIED,
        }
    }

    #[inline]
    pub unsafe fn init(info: &BootInfo, f: fn() -> ()) -> ! {
        let shared = Self::shared();
        shared.platform = info.platform;
        shared.cpu_ver = info.cpu_ver;
        // shared.acpi_rsdptr = info.acpi_rsdptr as usize;

        shared.main_screen = match info.screen_bpp {
            32 => {
                let size = Size::new(info.screen_width as isize, info.screen_height as isize);
                let stride = info.screen_stride as usize;
                let screen =
                    Bitmap32::from_static(info.vram_base as usize as *mut TrueColor, size, stride);
                Some(Bitmap::from(screen))
            }
            _ => {
                let size = Size::new(info.screen_width as isize, info.screen_height as isize);
                let stride = info.screen_stride as usize;
                let screen = Bitmap8::from_static(
                    info.vram_base as usize as *mut IndexedColor,
                    size,
                    stride,
                );
                Some(Bitmap::from(screen))
            }
        };

        mem::MemoryManager::init(&info);
        arch::Arch::init();

        task::scheduler::Scheduler::start(Self::late_init, f as usize);
    }

    fn late_init(f: usize) {
        unsafe {
            FontManager::init();
            window::WindowManager::init();
            io::hid::HidManager::init();
            arch::Arch::late_init();

            let f: fn() -> () = core::mem::transmute(f);
            f();
        }
    }

    /// Returns an internal shared instance
    #[inline]
    fn shared() -> &'static mut System {
        unsafe { &mut SYSTEM }
    }

    /// Returns the name of current system.
    #[inline]
    pub const fn name() -> &'static str {
        &Version::SYSTEM_NAME
    }

    /// Returns abbreviated name of current system.
    #[inline]
    pub const fn short_name() -> &'static str {
        &Version::SYSTEM_SHORT_NAME
    }

    /// Returns the version of current system.
    #[inline]
    pub const fn version() -> &'static Version {
        &Version::VERSION
    }

    /// Returns the current system time.
    #[inline]
    pub fn system_time() -> SystemTime {
        arch::Arch::system_time()
    }

    #[inline]
    pub fn platform() -> Platform {
        let shared = Self::shared();
        shared.platform
    }

    #[inline]
    pub fn cpu_ver() -> CpuVersion {
        let shared = Self::shared();
        shared.cpu_ver
    }

    /// SAFETY: IT DESTROYS EVERYTHING.
    pub unsafe fn reset() -> ! {
        Cpu::reset();
    }

    /// SAFETY: IT DESTROYS EVERYTHING.
    pub unsafe fn shutdown() -> ! {
        todo!();
    }

    /// Get main screen
    pub fn main_screen() -> &'static mut Bitmap<'static> {
        let shared = Self::shared();
        shared.main_screen.as_mut().unwrap()
    }

    /// Get emergency console
    pub fn em_console<'a>() -> &'a mut EmConsole {
        let shared = Self::shared();
        &mut shared.em_console
    }

    /// Get standard output
    pub fn stdout<'a>() -> &'a mut dyn Write {
        Self::em_console()
    }

    // TODO:
    // pub fn acpi() -> usize {
    //     let shared = Self::shared();
    //     shared.acpi_rsdptr
    // }
}

#[derive(Debug, Copy, Clone)]
pub struct SystemTime {
    pub secs: u64,
    pub nanos: u32,
}

// A Computer System

use crate::graphics::bitmap::*;
use crate::graphics::color::*;
use crate::graphics::coords::*;
use crate::graphics::emcon::*;
use crate::*;
use arch::cpu::Cpu;
use bootprot::*;
use core::fmt;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    versions: u32,
    rel: &'static str,
}

impl Version {
    const SYSTEM_NAME: &'static str = "codename TOE";
    const RELEASE: &'static str = "";
    const VERSION: Version = Version::new(0, 0, 1, Self::RELEASE);

    const fn new(maj: u8, min: u8, patch: u16, rel: &'static str) -> Self {
        let versions = ((maj as u32) << 24) | ((min as u32) << 16) | (patch as u32);
        Version { versions, rel }
    }

    pub const fn as_u32(&self) -> u32 {
        self.versions
    }

    pub const fn maj(&self) -> usize {
        ((self.versions >> 24) & 0xFF) as usize
    }

    pub const fn min(&self) -> usize {
        ((self.versions >> 16) & 0xFF) as usize
    }

    pub const fn patch(&self) -> usize {
        (self.versions & 0xFFFF) as usize
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
    main_screen: Option<OsMutBitmap8<'static>>,
    em_console: EmConsole,
    platform: Platform,
}

#[used]
static mut SYSTEM: System = System::new();

impl System {
    const fn new() -> Self {
        Self {
            main_screen: None,
            em_console: EmConsole::new(),
            platform: Platform::Unknown,
        }
    }

    #[inline]
    pub unsafe fn init(info: &BootInfo, f: fn() -> ()) -> ! {
        let shared = Self::shared();
        shared.platform = info.platform;

        let size = Size::new(info.screen_width as isize, info.screen_height as isize);
        let stride = info.screen_stride as usize;
        shared.main_screen = Some(OsMutBitmap8::from_static(
            info.vram_base as usize as *mut IndexedColor,
            size,
            stride,
        ));

        mem::mm::MemoryManager::init_first(&info);
        arch::Arch::init();

        window::WindowManager::init();
        io::hid::HidManager::init();

        f();

        loop {
            Cpu::halt();
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

    /// Returns the version of current system.
    #[inline]
    pub const fn version() -> &'static Version {
        &Version::VERSION
    }

    #[inline]
    pub fn platform() -> Platform {
        let shared = Self::shared();
        shared.platform
    }

    /// SAFETY: IT DESTROYS EVERYTHING.
    pub unsafe fn reset() -> ! {
        todo!();
    }

    /// SAFETY: IT DESTROYS EVERYTHING.
    pub unsafe fn shutdown() -> ! {
        todo!();
    }

    /// Get main screen
    pub fn main_screen() -> &'static mut OsMutBitmap8<'static> {
        let shared = Self::shared();
        shared.main_screen.as_mut().unwrap()
    }

    /// Get emergency console
    pub fn em_console<'a>() -> &'a mut EmConsole {
        let shared = Self::shared();
        &mut shared.em_console
    }

    // #[inline]
    // pub fn uarts<'a>() -> &'a [Box<dyn Uart>] {
    //     arch::Arch::uarts()
    // }
}

#[derive(Debug, Copy, Clone)]
pub struct SystemTime {
    pub secs: u64,
    pub nanos: u32,
}

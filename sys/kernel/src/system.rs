// A Computer System

use crate::{
    arch::cpu::Cpu,
    io::emcon::*,
    io::{null::Null, tty::Tty},
    *,
};
use alloc::boxed::Box;
use core::fmt;
use megstd::drawing::*;
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
    pub const fn new(maj: u8, min: u8, patch: u16, rel: &'static str) -> Self {
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
    main_screen: Option<OwnedBitmap<'static>>,
    em_console: EmConsole,
    stdout: Option<Box<dyn Tty>>,

    platform: Platform,
    cpu_ver: CpuVersion,
    initrd_base: usize,
    initrd_size: usize,
}

static mut SYSTEM: System = System::new();

impl System {
    const fn new() -> Self {
        Self {
            main_screen: None,
            em_console: EmConsole::new(ui::font::FontManager::fixed_system_font()),
            stdout: None,
            platform: Platform::Unknown,
            cpu_ver: CpuVersion::UNSPECIFIED,
            initrd_base: 0,
            initrd_size: 0,
        }
    }

    #[inline]
    pub unsafe fn init(info: &BootInfo, f: fn() -> ()) -> ! {
        let shared = Self::shared();
        shared.platform = info.platform;
        shared.cpu_ver = info.cpu_ver;
        shared.initrd_base = info.initrd_base as usize;
        shared.initrd_size = info.initrd_size as usize;
        // shared.acpi_rsdptr = info.acpi_rsdptr as usize;

        shared.main_screen = match info.screen_bpp {
            32 => Some(
                Bitmap32::from_static(
                    info.vram_base as usize as *mut TrueColor,
                    Size::new(info.screen_width as isize, info.screen_height as isize),
                    info.screen_stride as usize,
                )
                .into(),
            ),
            _ => Some(
                Bitmap8::from_static(
                    info.vram_base as usize as *mut IndexedColor,
                    Size::new(info.screen_width as isize, info.screen_height as isize),
                    info.screen_stride as usize,
                )
                .into(),
            ),
        };

        mem::MemoryManager::init_first(&info);

        arch::Arch::init();

        task::scheduler::Scheduler::start(Self::late_init, f as usize);
    }

    fn late_init(f: usize) {
        let shared = Self::shared();
        unsafe {
            mem::MemoryManager::late_init();

            fs::FileManager::init(shared.initrd_base, shared.initrd_size);

            rt::RuntimeEnvironment::init();

            ui::font::FontManager::init();

            ui::window::WindowManager::init();

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

    /// Returns whether the kernel is multiprocessor-capable.
    #[inline]
    pub const fn is_multi_processor_capable_kernel() -> bool {
        false
    }

    /// Returns the number of logical CPU cores.
    #[inline]
    pub fn num_of_cpus() -> usize {
        1
    }

    /// Returns the number of performance CPU cores.
    /// Returns less than `num_of_cpus` for SMT-enabled processors or heterogeneous computing.
    #[inline]
    pub fn num_of_performance_cpus() -> usize {
        1
    }

    /// Returns the number of active logical CPU cores.
    /// Returns the same value as `num_of_cpus` except during SMP initialization.
    #[inline]
    pub fn num_of_active_cpus() -> usize {
        1
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
    pub fn reset() -> ! {
        Cpu::reset();
    }

    /// SAFETY: IT DESTROYS EVERYTHING.
    pub fn shutdown() -> ! {
        todo!();
    }

    /// Get main screen
    pub fn main_screen() -> Bitmap<'static> {
        let shared = Self::shared();
        shared.main_screen.as_mut().unwrap().as_bitmap()
    }

    /// Get emergency console
    pub fn em_console<'a>() -> &'a mut EmConsole {
        let shared = Self::shared();
        &mut shared.em_console
    }

    pub fn set_stdout(stdout: Box<dyn Tty>) {
        let shared = Self::shared();
        shared.stdout = Some(stdout);
    }

    /// Get standard output
    pub fn stdout<'a>() -> &'a mut dyn Tty {
        let shared = Self::shared();
        shared
            .stdout
            .as_mut()
            .map(|v| v.as_mut())
            .unwrap_or(Null::null())
    }

    /// Get standard input
    pub fn stdin<'a>() -> &'a mut dyn Tty {
        Null::null()
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

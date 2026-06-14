//! MiniOS Execution Environment

use core::fmt;
use core::iter::Iterator;
use core::mem::MaybeUninit;
use core::ops::Range;
use core::panic::PanicInfo;
use core::ptr::NonNull;
use core::time::Duration;

pub use bootprot::PlatformType;
use guid::Guid;

use crate::io::fonts;
use crate::io::graphics::display::FbDisplay8;
use crate::io::graphics::fbcon::FbCon;
use crate::io::graphics::{GraphicsOutputDevice, PreferredGraphicsMode};
use crate::io::tty::{SimpleTextInput, SimpleTextOutput};
use crate::mem::MemoryManager;
use crate::null::NullTty;
use crate::platform::*;
use crate::task::event::{Event, PollResult};
use crate::*;

static mut SYSTEM: MaybeUninit<System> = MaybeUninit::zeroed();

static mut NULL: NullTty = NullTty {};

/// MiniOS Execution Environment
pub struct System {
    info: SsblInfo,
    config_table: Vec<ConfigurationTableEntry>,

    stdin: NonNull<dyn SimpleTextInput>,
    stdout: NonNull<dyn SimpleTextOutput>,
    console_controller: ConsoleController,

    device_tree: Option<fdt::DeviceTree<'static>>,
}

/// Configuration table entry
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ConfigurationTableEntry {
    pub guid: Guid,
    pub address: NonNullPhysicalAddress,
}

impl System {
    pub const DEFAULT_STDOUT_ATTRIBUTE: u8 = 0x07;

    /// Initialize with SSBL info
    #[inline]
    pub unsafe fn init(info: &SsblInfo, arg: usize, main: fn() -> ()) -> ! {
        unsafe {
            let env = System {
                info: info.clone(),
                config_table: Vec::new(),
                stdin: NonNull::new(&raw mut NULL).unwrap(),
                stdout: NonNull::new(&raw mut NULL).unwrap(),
                console_controller: ConsoleController::new(),
                device_tree: None,
            };

            (&mut *(&raw mut SYSTEM)).write(env);

            MemoryManager::init();

            Platform::init(arg);
        }
        Self::_init(main)
    }

    /// Initialize with device tree blob
    #[cfg(feature = "device_tree")]
    #[inline]
    pub unsafe fn init_dt(dtb: usize, arg: usize, main: fn() -> ()) -> ! {
        unsafe {
            let mut shared = System {
                info: SsblInfo {
                    platform_type: PlatformType::DeviceTree,
                    bios_boot_drive: BiosDriveSpec(0),
                    x86_real_memory_size: 0,
                    reserved: 0,
                    start_conventional_memory: 0,
                    conventional_memory_size: 0,
                },
                config_table: Vec::new(),
                stdin: NonNull::new(&raw mut NULL).unwrap(),
                stdout: NonNull::new(&raw mut NULL).unwrap(),
                console_controller: ConsoleController::new(),
                device_tree: None,
            };
            shared.device_tree = fdt::DeviceTree::parse(dtb as *const u8).ok();
            (&mut *(&raw mut SYSTEM)).write(shared);

            let dt = Self::device_tree().unwrap();

            Platform::init_dt_early(&dt, arg);

            MemoryManager::init_dt(&dt);

            if let Some(dt) = NonNullPhysicalAddress::from_ptr(dt.as_ptr()) {
                System::add_config_table_entry(&fdt::DTB_TABLE_GUID, dt);
            }

            Platform::init(arg);
        }
        Self::_init(main)
    }

    /// Initialize in UEFI environment
    #[cfg(feature = "uefi")]
    #[inline]
    pub unsafe fn init_uefi(arg: usize, main: fn() -> ()) -> ! {
        unsafe {
            let shared = System {
                info: SsblInfo {
                    platform_type: PlatformType::UefiNative,
                    bios_boot_drive: BiosDriveSpec(0),
                    x86_real_memory_size: 0,
                    reserved: 0,
                    start_conventional_memory: 0,
                    conventional_memory_size: 0,
                },
                config_table: Vec::new(),
                stdin: NonNull::new(&raw mut NULL).unwrap(),
                stdout: NonNull::new(&raw mut NULL).unwrap(),
                console_controller: ConsoleController::new(),
                device_tree: None,
            };
            (&mut *(&raw mut SYSTEM)).write(shared);

            Platform::init(arg);
        }
        Self::_init(main)
    }

    #[inline(always)]
    fn _init(main: fn() -> ()) -> ! {
        main();

        panic!("The system has halted");
    }

    #[inline]
    fn shared<'a>() -> &'a Self {
        unsafe { (&*(&raw mut SYSTEM)).assume_init_ref() }
    }

    #[inline]
    unsafe fn shared_mut<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut SYSTEM)).assume_init_mut() }
    }

    /// Returns boot information
    #[inline]
    pub fn boot_info<'a>() -> &'a SsblInfo {
        let shared = Self::shared();
        &shared.info
    }

    /// Returns boot information (mutable)
    #[inline]
    pub unsafe fn boot_info_mut<'a>() -> &'a mut SsblInfo {
        unsafe {
            let shared = Self::shared_mut();
            &mut shared.info
        }
    }

    /// Returns current platform type
    #[inline]
    pub fn platform_type() -> PlatformType {
        Self::boot_info().platform_type
    }

    /// Returns device tree if available
    #[inline]
    pub fn device_tree<'a>() -> Option<&'a fdt::DeviceTree<'a>> {
        let shared = Self::shared();
        shared.device_tree.as_ref()
    }

    /// # Safety
    ///
    /// After calling this function, all minios functions will cease to function.
    pub unsafe fn exit_minios() {
        unsafe {
            // let shared = Self::shared_mut();

            Platform::exit();

            *(&mut *(&raw mut SYSTEM)) = MaybeUninit::zeroed();
        }
    }

    /// Get current stdin
    #[inline]
    pub fn stdin<'a>() -> &'a mut dyn SimpleTextInput {
        unsafe {
            let shared = Self::shared_mut();
            shared.stdin.as_mut()
        }
    }

    /// Get current stdout
    #[inline]
    pub fn stdout<'a>() -> &'a mut dyn SimpleTextOutput {
        unsafe {
            let shared = Self::shared_mut();
            shared.stdout.as_mut()
        }
    }

    /// Returns console controller
    #[inline]
    pub fn conctl<'a>() -> &'a mut ConsoleController {
        unsafe {
            let shared = Self::shared_mut();
            &mut shared.console_controller
        }
    }

    /// Wait for any of the events to be signaled and returns the signaled event.
    #[inline(never)]
    pub fn wait_for_events<'a, 'b, 'c>(
        events: &'a mut [&'b mut Event<'c>],
    ) -> &'a mut &'b mut Event<'c> {
        let index = 'main: loop {
            for (i, event) in events.iter_mut().enumerate() {
                match event.poll() {
                    PollResult::Ready => {
                        break 'main i;
                    }
                    PollResult::Pending => {}
                }
            }
            Hal::cpu().wait_for_interrupt();
        };
        events.get_mut(index).unwrap()
    }

    /// Returns configuration table entries
    #[inline]
    pub fn config_table<'a>() -> &'a [ConfigurationTableEntry] {
        let shared = Self::shared();
        shared.config_table.as_slice()
    }

    /// Finds configuration table entry by GUID
    #[inline]
    pub fn find_config_table_entry(guid: &Guid) -> Option<&'static ConfigurationTableEntry> {
        let shared = Self::shared();
        for entry in &shared.config_table {
            if &entry.guid == guid {
                return Some(entry);
            }
        }
        None
    }

    /// Adds a configuration table entry
    #[inline]
    pub unsafe fn add_config_table_entry(guid: &Guid, address: NonNullPhysicalAddress) {
        unsafe {
            let shared = Self::shared_mut();
            shared.config_table.push(ConfigurationTableEntry {
                guid: *guid,
                address,
            });
        }
    }

    #[inline]
    pub unsafe fn set_stdin(stdin: &'static mut dyn SimpleTextInput) {
        unsafe {
            let shared = Self::shared_mut();
            shared.stdin = NonNull::new_unchecked(stdin);
        }
    }

    #[inline]
    pub unsafe fn set_stdout(stdout: &'static mut dyn SimpleTextOutput) {
        unsafe {
            let shared = Self::shared_mut();
            shared.stdout = NonNull::new_unchecked(stdout);
            shared.console_controller.text_out = NonNull::new_unchecked(stdout);
        }
    }

    #[inline]
    unsafe fn _set_stdout(stdout: &'static mut dyn SimpleTextOutput) {
        unsafe {
            let shared = Self::shared_mut();
            shared.stdout = NonNull::new_unchecked(stdout);
        }
    }

    /// Helper function to convert a duration to timer ticks.
    pub fn duration_to_ticks_helper32(duration: Duration, nanos_per_tick: u32) -> u64 {
        let nanos = duration.subsec_nanos();
        let ticks_nanos = ((nanos + nanos_per_tick - 1) / nanos_per_tick).max(1);

        let ticks_per_sec = (1_000_000_000 / nanos_per_tick) as u64;
        let secs = duration.as_secs();
        let ticks_secs = if secs == 0 {
            0
        } else {
            secs.saturating_mul(ticks_per_sec)
        };

        ticks_secs.saturating_add(ticks_nanos as u64)
    }

    /// Sets graphics mode if the platform recommends graphics mode
    pub fn set_graphics_mode_if_recommended() {
        if Platform::recommended_console_mode() == RecommendedConsoleMode::Graphics
            && let Some(mode) = Self::conctl().preferred_graphics_mode()
        {
            let _ = Self::conctl().set_graphics_mode(mode);
        }
    }
}

/// Panic handler
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let stdout = System::stdout();
    stdout.set_attribute(0xcf);
    println!("{}", info);

    loop {
        Hal::cpu().halt();
    }
}

/// Boot information from Second Stage Boot Loader
#[repr(C)]
#[derive(Debug, Clone)]
pub struct SsblInfo {
    /// Platform type
    pub platform_type: PlatformType,
    /// BIOS boot drive secifier (for x86 PC platforms)
    pub bios_boot_drive: BiosDriveSpec,
    /// Real memory size in paragraphs (for x86 PC platforms)
    pub x86_real_memory_size: u16,
    /// Reserved
    pub reserved: u32,
    /// Start address of conventional memory
    pub start_conventional_memory: u32,
    /// Size of conventional memory in bytes
    pub conventional_memory_size: u32,
}

impl SsblInfo {
    #[inline]
    pub fn conventional_memory_range(&self) -> Range<u64> {
        self.start_conventional_memory as u64
            ..(self.start_conventional_memory + self.conventional_memory_size) as u64
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BiosDriveSpec(pub u8);

impl fmt::Debug for BiosDriveSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BiosDriveSpec(0x{:02x})", self.0)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version<'a> {
    versions: u32,
    rel: &'a str,
}

impl Version<'_> {
    #[inline]
    pub const fn new<'a>(maj: u8, min: u8, patch: u16, rel: &'a str) -> Version<'a> {
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
    pub const fn rel(&self) -> &str {
        &self.rel
    }
}

impl fmt::Display for Version<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.rel().len() > 0 {
            write!(
                f,
                "{}.{}.{}-{}",
                self.maj(),
                self.min(),
                self.patch(),
                self.rel(),
            )
        } else if self.patch() > 0 {
            write!(f, "{}.{}.{}", self.maj(), self.min(), self.patch(),)
        } else {
            write!(f, "{}.{}", self.maj(), self.min(),)
        }
    }
}

/// Console Controller
pub struct ConsoleController {
    is_text_mode: bool,
    text_out: NonNull<dyn SimpleTextOutput>,
    graphics_out: Option<Box<dyn GraphicsOutputDevice>>,
    fbcon: Option<FbCon>,
    preferred_graphics_mode: Option<PreferredGraphicsMode>,
}

impl ConsoleController {
    #[inline]
    const fn new() -> Self {
        Self {
            is_text_mode: true,
            text_out: NonNull::new(&raw mut NULL).unwrap(),
            graphics_out: None,
            fbcon: None,
            preferred_graphics_mode: None,
        }
    }

    /// Set graphics output device.
    #[inline]
    pub fn set_graphics(&mut self, graphics_out: Box<dyn GraphicsOutputDevice>) {
        self.set_text_mode();
        self.graphics_out = Some(graphics_out);
    }

    #[inline]
    pub fn set_preferred_graphics_mode(&mut self, mode: PreferredGraphicsMode) {
        self.preferred_graphics_mode = Some(mode);
    }

    #[inline]
    pub const fn preferred_graphics_mode(&self) -> Option<PreferredGraphicsMode> {
        self.preferred_graphics_mode
    }

    /// Returns whether the console is in text mode
    #[inline]
    pub const fn is_text_mode(&self) -> bool {
        self.is_text_mode
    }

    /// Returns whether the console is in graphics mode
    #[inline]
    pub const fn is_graphics_mode(&self) -> bool {
        !self.is_text_mode()
    }

    /// Sets text mode
    pub fn set_text_mode(&mut self) {
        if !self.is_text_mode {
            if let Some(graphics) = self.graphics_out.as_mut() {
                graphics.detach();
                unsafe {
                    System::_set_stdout(self.text_out.as_mut());
                }
                self.fbcon = None;
            }
            self.is_text_mode = true;

            System::stdout().reset();
        }
    }

    /// Returns current graphics mode if in graphics mode
    pub fn current_graphics_mode(&self) -> Option<&io::graphics::CurrentMode> {
        (self.is_graphics_mode())
            .then(|| self.graphics_out.as_ref())
            .flatten()
            .map(|v| v.current_mode())
    }

    /// Returns current draw target if in graphics mode
    pub fn current_draw_target(&mut self) -> Option<&mut FbDisplay8> {
        (self.is_graphics_mode())
            .then(|| self.fbcon.as_mut())
            .flatten()
            .map(|v| v.current_fb())
    }

    /// Sets graphics mode by index
    pub fn set_graphics_mode_by_index(&mut self, mode: io::graphics::ModeIndex) -> Result<(), ()> {
        let Some(graphics) = self.graphics_out.as_mut() else {
            return Err(());
        };
        let prev_is_text_mode = self.is_text_mode;
        let prev_graphics_mode = graphics.current_mode().current;

        let mode_info = graphics.modes().get(mode.0).ok_or(())?.clone();
        if !FbDisplay8::is_supported_pixel_format(mode_info.pixel_format) {
            return Err(());
        }

        graphics.set_mode(mode)?;

        unsafe {
            let current_mode = graphics.current_mode();
            let display = match FbDisplay8::from_graphics(current_mode) {
                Some(d) => d,
                None => {
                    // fallback
                    if prev_is_text_mode {
                        System::_set_stdout(self.text_out.as_mut());
                    } else {
                        graphics
                            .set_mode(prev_graphics_mode)
                            .expect("Failed to restore previous graphics mode");
                    }
                    return Err(());
                }
            };

            System::_set_stdout(&mut *(&raw mut NULL));

            let font = fonts::preferred_font_for(
                current_mode.info.width as u32,
                current_mode.info.height as u32,
            );
            self.fbcon = FbCon::new(display, font).into();

            // SAFETY: to avoid lifetime
            System::_set_stdout(core::mem::transmute(
                self.fbcon.as_mut().unwrap() as &mut dyn SimpleTextOutput
            ));
        }

        self.is_text_mode = false;

        System::stdout().reset();

        Ok(())
    }

    /// Finds graphics mode index by resolution and pixel format
    pub fn find_graphics_mode(
        &self,
        mode: PreferredGraphicsMode,
    ) -> Option<io::graphics::ModeIndex> {
        let graphics = self.graphics_out.as_ref()?;
        for (index, item) in graphics.modes().iter().enumerate() {
            if item.width == mode.width
                && item.height == mode.height
                && item.pixel_format == mode.pixel_format
            {
                return Some(io::graphics::ModeIndex(index));
            }
        }
        None
    }

    /// Sets the best graphics mode matching the given criteria
    #[inline]
    pub fn set_graphics_mode(&mut self, mode: PreferredGraphicsMode) -> Result<(), ()> {
        let mode = self.find_graphics_mode(mode).ok_or(())?;
        self.set_graphics_mode_by_index(mode)
    }

    /// Sets the best graphics mode from the given list of candidates.
    /// The candidates should be ordered by priority.
    pub fn set_graphics_mode_from_list(
        &mut self,
        candidates: &[PreferredGraphicsMode],
    ) -> Result<usize, ()> {
        for (i, mode) in candidates.iter().copied().enumerate() {
            if let Some(mode) = self.find_graphics_mode(mode) {
                match self.set_graphics_mode_by_index(mode) {
                    Ok(()) => return Ok(i),
                    Err(()) => continue,
                }
            }
        }
        Err(())
    }
}

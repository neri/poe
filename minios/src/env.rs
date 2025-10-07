//! MiniOS Execution Environment

use crate::{null::NullTty, platform::*, *};
use core::{fmt, iter::Iterator, mem::MaybeUninit, ops::Range, panic::PanicInfo, ptr::NonNull};
use guid::Guid;
use io::tty::{SimpleTextInput, SimpleTextOutput};
use mem::MemoryManager;

static mut SYSTEM: MaybeUninit<System> = MaybeUninit::zeroed();

static mut NULL: NullTty = NullTty {};

/// MiniOS Execution Environment
pub struct System {
    info: BootInfo,
    config_table: Vec<ConfigurationTableEntry>,

    stdin: NonNull<dyn SimpleTextInput>,
    stdout: NonNull<dyn SimpleTextOutput>,
    stderr: NonNull<dyn SimpleTextOutput>,

    smbios: Option<smbios::SmBios>,
    device_tree: Option<fdt::DeviceTree<'static>>,
}

impl System {
    pub const DEFAULT_STDOUT_ATTRIBUTE: u8 = 0x07; //0x1f;

    /// Initialize with boot information and main function
    #[inline]
    pub unsafe fn init(info: &BootInfo, arg: usize, main: fn() -> ()) -> ! {
        unsafe {
            let env = System {
                info: info.clone(),
                config_table: Vec::new(),
                stdin: NonNull::new_unchecked(&raw mut NULL),
                stdout: NonNull::new_unchecked(&raw mut NULL),
                stderr: NonNull::new_unchecked(&raw mut NULL),
                smbios: None,
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
                info: BootInfo {
                    platform: Platform::DeviceTree,
                    bios_boot_drive: BiosDriveSpec(0),
                    x86_real_memory_size: 0,
                    reserved_memory_size: 0,
                    start_conventional_memory: 0,
                    conventional_memory_size: 0,
                },
                config_table: Vec::new(),
                stdin: NonNull::new_unchecked(&raw mut NULL),
                stdout: NonNull::new_unchecked(&raw mut NULL),
                stderr: NonNull::new_unchecked(&raw mut NULL),
                smbios: None,
                device_tree: None,
            };
            shared.device_tree = fdt::DeviceTree::parse(dtb as *const u8).ok();
            (&mut *(&raw mut SYSTEM)).write(shared);

            let dt = Self::device_tree().unwrap();

            Platform::init_dt_early(&dt, arg);

            MemoryManager::init_dt(&dt);

            if let Some(dt) = NonNullPhysicalAddress::from_ptr(dt.as_ptr()) {
                System::add_config_table_entry(fdt::DTB_TABLE_GUID, dt);
            }

            Platform::init(arg);
        }
        Self::_init(main)
    }

    #[inline(always)]
    fn _init(main: fn() -> ()) -> ! {
        unsafe {
            let shared = Self::shared_mut();

            if let Some(item) = Self::find_config_table_entry(&smbios::SMBIOS_GUID) {
                let smbios = smbios::SmBios::parse(
                    item.address.get().as_usize() as *const core::ffi::c_void
                );
                shared.smbios = smbios;
            }
        }

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

    #[inline]
    pub fn boot_info<'a>() -> &'a BootInfo {
        let shared = Self::shared();
        &shared.info
    }

    #[inline]
    pub unsafe fn boot_info_mut<'a>() -> &'a mut BootInfo {
        unsafe {
            let shared = Self::shared_mut();
            &mut shared.info
        }
    }

    #[inline]
    pub fn platform() -> Platform {
        Self::boot_info().platform
    }

    #[inline]
    pub fn smbios<'a>() -> Option<&'a smbios::SmBios> {
        let shared = Self::shared();
        shared.smbios.as_ref()
    }

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

    #[inline]
    pub fn stdin<'a>() -> &'a mut dyn SimpleTextInput {
        unsafe {
            let shared = Self::shared_mut();
            shared.stdin.as_mut()
        }
    }

    #[inline]
    pub fn stdout<'a>() -> &'a mut dyn SimpleTextOutput {
        unsafe {
            let shared = Self::shared_mut();
            shared.stdout.as_mut()
        }
    }

    #[inline]
    pub fn stderr<'a>() -> &'a mut dyn SimpleTextOutput {
        unsafe {
            let shared = Self::shared_mut();
            shared.stderr.as_mut()
        }
    }

    pub fn line_input(max_len: usize) -> Option<String> {
        let mut buf = Vec::with_capacity(max_len);
        let stdin = Self::stdin();
        let stdout = Self::stdout();

        loop {
            stdout.enable_cursor(true);
            match stdin.read_key_stroke() {
                Some(key) => {
                    stdout.enable_cursor(false);
                    let key = key.get();
                    let c = key.unicode_char as u8 as char;
                    match c {
                        // ctrl-c
                        '\x03' => {
                            return None;
                        }
                        // backspace
                        '\x08' | '\x7f' => match buf.pop() {
                            Some(c) => {
                                if c < ' ' {
                                    stdout.write_str("\x08\x08  \x08\x08").unwrap();
                                } else {
                                    stdout.write_str("\x08 \x08").unwrap();
                                }
                            }
                            None => {}
                        },
                        // enter
                        '\x0a' | '\x0d' => {
                            stdout.write_str("\r\n").unwrap();
                            break;
                        }
                        _ => {
                            if buf.len() < max_len {
                                if c < ' ' {
                                    // control char
                                    stdout.write_char('^').unwrap();
                                    stdout.write_char((c as u8 | 0x40) as char).unwrap();
                                    buf.push(c);
                                } else if c <= '\x7E' {
                                    // printable char
                                    let _ = stdout.write_char(c);
                                    buf.push(c);
                                } else {
                                    // TODO: unprintable char
                                }
                            }
                        }
                    }
                }
                None => {
                    // assert!(unsafe { Hal::cpu().is_interrupt_enabled() });
                    Hal::cpu().wait_for_interrupt();
                }
            }
        }
        Some(buf.into_iter().collect())
    }

    #[inline]
    pub fn config_table<'a>() -> impl Iterator<Item = &'a ConfigurationTableEntry> {
        let shared = Self::shared();
        shared.config_table.iter()
    }

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

    #[inline]
    pub unsafe fn add_config_table_entry(guid: Guid, address: NonNullPhysicalAddress) {
        unsafe {
            let shared = Self::shared_mut();
            shared
                .config_table
                .push(ConfigurationTableEntry { guid, address });
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
        }
    }

    #[inline]
    pub unsafe fn set_stderr(stderr: &'static mut dyn SimpleTextOutput) {
        unsafe {
            let shared = Self::shared_mut();
            shared.stderr = NonNull::new_unchecked(stderr);
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let stderr = System::stderr();
    stderr.set_attribute(0xcf);
    let _ = writeln!(stderr, "{}", info);
    loop {
        Hal::cpu().halt();
    }
}

/// Boot information from Second Stage Boot Loader
#[repr(C)]
#[derive(Debug, Clone)]
pub struct BootInfo {
    pub platform: Platform,
    pub bios_boot_drive: BiosDriveSpec,
    pub x86_real_memory_size: u16,
    pub reserved_memory_size: u32,
    pub start_conventional_memory: u32,
    pub conventional_memory_size: u32,
}

impl BootInfo {
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

#[repr(C)]
#[derive(Debug, Clone)]
pub struct ConfigurationTableEntry {
    pub guid: Guid,
    pub address: NonNullPhysicalAddress,
}

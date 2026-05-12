//! Platform dependent module for RISC-V machine with SBI

use super::*;
use crate::*;
use core::cell::UnsafeCell;
use core::{ffi::c_void, time::Duration};
use sbi::Eid;

pub mod sbi_console;
pub mod timer;
pub mod trap;

unsafe extern "C" {
    unsafe static _end: c_void;
}

impl PlatformTrait for Platform {
    unsafe fn init_dt_early(dt: &fdt::DeviceTree, arg: usize) {
        let hart_id = arg;
        unsafe {
            sbi_console::SbiConsole::init();
            System::set_stdin(sbi_console::SbiConsole::shared());
            System::set_stdout(sbi_console::SbiConsole::shared());

            CurrentConfig::shared().is_system_reset_supported =
                sbi::base::probe_extension(Eid::SYSTEM_RESET).unwrap_or(false);

            trap::init();

            let boot_info = System::boot_info_mut();
            boot_info.platform = Platform::Sbi;

            let end = PhysicalAddress::new(&_end as *const _ as PhysicalAddressRepr);
            boot_info.start_conventional_memory = end.rounding_up_4k().as_repr() as u32;
            boot_info.conventional_memory_size = 0x40_0000;

            timer::PlatformTimer::init(dt);

            println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");

            let spec_ver = sbi::base::get_spec_version();
            let impl_id = sbi::base::get_impl_id().unwrap();
            let impl_ver = sbi::base::get_impl_version().unwrap();
            println!(
                "SBI version {}.{} impl {:?} version {:x}",
                spec_ver.major(),
                spec_ver.minor(),
                impl_id,
                impl_ver
            );

            println!("Hart ID: {}", hart_id);

            println!("Model: {}", dt.root().model());
            for item in dt.root().compatible().unwrap() {
                println!("compatible: {}", item);
            }
        }
    }

    unsafe fn init(_arg: usize) {
        unsafe {
            println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");

            Hal::cpu().enable_interrupt();
        }
    }

    unsafe fn exit() {
        // to do nothing for now
    }

    fn reset_system() -> ! {
        if CurrentConfig::shared().is_system_reset_supported() {
            sbi::system_reset_no_reason(sbi::ResetType::ColdReset).ok();
        }
        sbi::legacy::shutdown();
    }

    fn halt() -> ! {
        if CurrentConfig::shared().is_system_reset_supported() {
            sbi::system_reset_no_reason(sbi::ResetType::Shutdown).ok();
        }
        sbi::legacy::shutdown();
    }

    #[inline]
    fn monotonic() -> u64 {
        timer::PlatformTimer::monotonic()
    }

    #[inline]
    fn duration_to_ticks(duration: Duration) -> u64 {
        timer::PlatformTimer::duration_to_ticks(duration)
    }
}

static mut CURRENT_CONFIG: UnsafeCell<CurrentConfig> = UnsafeCell::new(CurrentConfig::new());

struct CurrentConfig {
    is_system_reset_supported: bool,
}

impl CurrentConfig {
    #[inline]
    const fn new() -> Self {
        Self {
            is_system_reset_supported: false,
        }
    }

    #[inline]
    fn shared() -> &'static mut Self {
        unsafe { (&mut *(&raw mut CURRENT_CONFIG)).get_mut() }
    }

    #[inline]
    fn is_system_reset_supported(&self) -> bool {
        self.is_system_reset_supported
    }
}

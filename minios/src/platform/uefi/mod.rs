//! Platform modules for UEFI

use crate::*;
use core::time::Duration;
use uefi::{Status, runtime::ResetType};

pub mod console;
pub mod gop;

impl PlatformTrait for Platform {
    unsafe fn init(_arg: usize) {
        unsafe {
            uefi::helpers::init().unwrap();

            console::UefiConsole::init();
            gop::UefiGop::init();
        }
    }

    unsafe fn exit() {
        unsafe {
            let _mmap = uefi::boot::exit_boot_services(None);
        }
    }

    fn reset_system() -> ! {
        uefi::runtime::reset(ResetType::COLD, Status::SUCCESS, None);
    }

    fn halt() -> ! {
        uefi::runtime::reset(ResetType::SHUTDOWN, Status::SUCCESS, None);
    }

    fn monotonic() -> u64 {
        // TODO: uefi does not provide a monotonic timer
        // NOTE: `GetNextMonotonicCount` is not suitable for this purpose.
        0
    }

    fn duration_to_ticks(_duration: Duration) -> u64 {
        todo!()
    }
}

//! Platform modules for UEFI

use crate::platform::RecommendedConsoleMode;
use crate::*;
use core::mem::transmute;
use core::time::Duration;
use uefi::{Status, runtime::ResetType};

pub mod console;
pub mod event;
pub mod gop;

impl PlatformTrait for Platform {
    unsafe fn init(_arg: usize) {
        unsafe {
            uefi::helpers::init().unwrap();

            console::UefiConsole::init();

            uefi::system::with_config_table(|items| {
                for item in items {
                    let guid = transmute(item.guid);
                    System::add_config_table_entry(
                        &guid,
                        NonNullPhysicalAddress::from_ptr(item.address).unwrap(),
                    );
                }
            });

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

    fn create_timer_event(duration: Duration) -> Box<dyn PollingEvent> {
        Box::new(event::EfiEventPoller::create_timer(duration))
    }

    fn recommended_console_mode() -> RecommendedConsoleMode {
        RecommendedConsoleMode::Graphics
    }
}

//! Platform modules for UEFI

use core::mem::transmute;
use core::time::Duration;

use uefi::Status;
use uefi::runtime::ResetType;

use crate::platform::RecommendedConsoleMode;
use crate::*;

pub mod block;
pub mod console;
pub mod device_path;
pub mod event;
pub mod gop;

impl Platform for CurrentPlatform {
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

            block::BlockDeviceManager::init();

            gop::UefiGop::init();
        }
    }

    unsafe fn exit() {
        unsafe {
            let _mmap = uefi::boot::exit_boot_services(None);

            todo!()
        }
    }

    fn reset_system() -> ! {
        uefi::runtime::reset(ResetType::COLD, Status::SUCCESS, None);
    }

    fn halt() -> ! {
        uefi::runtime::reset(ResetType::SHUTDOWN, Status::SUCCESS, None);
    }

    fn monotonic() -> u64 {
        // TODO: UEFI does not provide a monotonic timer
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

/// Helper function to get a protocol interface from a handle
unsafe fn get_protocol<PROTOCOL: uefi::proto::ProtocolPointer + ?Sized>(
    handle: uefi::Handle,
) -> uefi::Result<uefi::boot::ScopedProtocol<PROTOCOL>> {
    unsafe {
        uefi::boot::open_protocol(
            uefi::boot::OpenProtocolParams {
                handle: handle,
                agent: uefi::boot::image_handle(),
                controller: None,
            },
            uefi::boot::OpenProtocolAttributes::GetProtocol,
        )
    }
}

// To avoid link error
#[unsafe(no_mangle)]
pub extern "C" fn wcslen(s: *const u16) -> usize {
    let mut len = 0;
    unsafe {
        while *s.add(len) != 0 {
            len += 1;
        }
    }
    len
}

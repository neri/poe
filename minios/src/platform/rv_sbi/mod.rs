//! Platform dependent module for RISC-V machine with SBI

use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::time::Duration;

use sbi::Eid;

use super::*;

#[cfg(target_arch = "riscv64")]
mod jh7110;
mod memory;
#[cfg(all(target_arch = "riscv64", feature = "virtio"))]
pub(crate) mod plic;
pub mod sbi_console;
pub mod timer;
pub mod trap;
#[cfg(all(feature = "usb", target_arch = "riscv64"))]
mod usb;

unsafe extern "C" {
    unsafe static _end: c_void;
}

impl Platform for CurrentPlatform {
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
            boot_info.platform_type = PlatformType::Sbi;

            let kernel_end = &_end as *const _ as u64;
            let (start, size) = memory::early_ram(dt, kernel_end)
                .expect("no safe 32-bit RAM span after the RISC-V kernel");
            boot_info.start_conventional_memory = start;
            boot_info.conventional_memory_size = size;

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
            #[cfg(target_arch = "riscv64")]
            jh7110::init_early(dt);
        }
    }

    unsafe fn init(_arg: usize) {
        unsafe {
            println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");

            #[cfg(feature = "virtio")]
            if let Some(dt) = System::device_tree() {
                crate::io::virtio::init(dt);
            }

            #[cfg(all(feature = "usb", target_arch = "riscv64"))]
            if let Some(dt) = System::device_tree()
                && dt.root().is_compatible_with("riscv-virtio")
                && let Err(reason) = usb::init(dt)
            {
                println!("USB disabled: {}", reason);
            }

            #[cfg(target_arch = "riscv64")]
            if let Some(dt) = System::device_tree() {
                jh7110::init(dt);
            }

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

    fn recommended_console_mode() -> RecommendedConsoleMode {
        #[cfg(feature = "virtio")]
        if crate::io::virtio::gpu::available() {
            return RecommendedConsoleMode::Graphics;
        }
        #[cfg(target_arch = "riscv64")]
        if System::device_tree().is_some_and(jh7110::is_visionfive2) {
            return RecommendedConsoleMode::Graphics;
        }
        RecommendedConsoleMode::None
    }

    #[inline]
    fn monotonic() -> u64 {
        timer::PlatformTimer::monotonic()
    }

    #[inline]
    fn create_timer_event(duration: Duration) -> Box<dyn PollingEvent> {
        let timeout = Self::monotonic() + timer::PlatformTimer::duration_to_ticks(duration);
        Box::new(MonotonicTimerPoller::Timeout(timeout))
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

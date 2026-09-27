//! Platform-specific code.

use core::time::Duration;

use crate::*;

#[cfg(feature = "pc")]
pub mod x86_pc;
#[cfg(feature = "pc")]
pub use x86_pc as current;

#[cfg(feature = "sbi")]
pub mod rv_sbi;
#[cfg(feature = "sbi")]
pub use rv_sbi as current;

#[cfg(feature = "arm64dt")]
pub mod arm64dt;
#[cfg(feature = "arm64dt")]
pub use arm64dt as current;

#[cfg(feature = "uefi")]
pub mod uefi;
#[cfg(feature = "uefi")]
pub use uefi as current;

pub struct CurrentPlatform;

pub trait Platform {
    /// Initialize platform with device tree and other early initialization.
    #[cfg(feature = "device_tree")]
    unsafe fn init_dt_early(dt: &fdt::DeviceTree, arg: usize);

    /// Adds the physical memory in use before the memory manager starts that
    /// the device tree does not describe: the MiniOS image (code, data, BSS and
    /// the boot stack), and things like a framebuffer the firmware left on.
    #[cfg(feature = "device_tree")]
    unsafe fn dt_memory_in_use(
        dt: &fdt::DeviceTree,
        builder: &mut crate::mem::EarlyMapBuilder,
    ) -> Result<(), crate::mem::MapError>;

    /// Initialize platform
    unsafe fn init(arg: usize);

    /// Exit platform
    unsafe fn exit();

    /// Reset the system. This function never returns.
    fn reset_system() -> !;

    /// Halt the system. This function never returns.
    fn halt() -> ! {
        Hal::cpu().halt();
    }

    /// Get monotonic timer value.
    fn monotonic() -> u64;

    /// Convert a duration to timer ticks.
    fn create_timer_event(duration: Duration) -> Box<dyn PollingEvent>;

    /// Returns the recommended console mode for this platform.
    fn recommended_console_mode() -> RecommendedConsoleMode {
        RecommendedConsoleMode::None
    }
}

#[cfg(not(any(feature = "pc", feature = "sbi", feature = "arm64dt", feature = "uefi")))]
impl Platform for CurrentPlatform {
    #[cfg(feature = "device_tree")]
    unsafe fn init_dt_early(_dt: &fdt::DeviceTree, _arg: usize) {}
    #[cfg(feature = "device_tree")]
    unsafe fn dt_memory_in_use(
        _dt: &fdt::DeviceTree,
        _builder: &mut crate::mem::EarlyMapBuilder,
    ) -> Result<(), crate::mem::MapError> {
        Ok(())
    }
    unsafe fn init(_arg: usize) {}
    unsafe fn exit() {}
    fn reset_system() -> ! {
        panic!("reset_system in host test")
    }
    fn halt() -> ! {
        panic!("halt in host test")
    }
    fn monotonic() -> u64 {
        0
    }
    fn create_timer_event(_duration: Duration) -> Box<dyn PollingEvent> {
        Box::new(MonotonicTimerPoller::Timeout(0))
    }
}

/// Recommended console mode for a platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecommendedConsoleMode {
    /// No recommendation
    None,
    /// Text mode is highly recommended.
    Text,
    /// Graphics mode is recommended.
    Graphics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonotonicTimerPoller {
    Timeout(u64),
}

impl PollingEvent for MonotonicTimerPoller {
    fn poll(&mut self) -> PollResult {
        match self {
            Self::Timeout(deadline) => {
                let result = CurrentPlatform::monotonic().wrapping_sub(*deadline) as i64;
                if result >= 0 {
                    PollResult::Ready
                } else {
                    PollResult::Pending
                }
            }
        }
    }
}

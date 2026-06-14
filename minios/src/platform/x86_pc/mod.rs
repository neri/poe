//! Platform modules for x86-pc
pub mod fm_towns;
pub mod ibm_pc;
pub mod nec98;

mod pic;
mod pit;

use core::time::Duration;

use super::{MonotonicTimerPoller, Platform, PlatformTrait};
use crate::arch::{cpu, gdt, idt, lomem, vm86};
use crate::mem::{MemoryManager, MemoryType};
use crate::*;

impl PlatformTrait for Platform {
    unsafe fn init(_arg: usize) {
        unsafe {
            let info = System::boot_info();

            cpu::Cpu::init();
            lomem::LoMemoryManager::init();

            MemoryManager::register_memmap(
                0x10_0000..info.start_conventional_memory as u64,
                MemoryType::Used,
            )
            .unwrap();

            match info.platform_type {
                PlatformType::Nec98 => {
                    nec98::init(&info);
                }
                PlatformType::PcBios => {
                    ibm_pc::init(&info);
                }
                PlatformType::FmTowns => {
                    fm_towns::init(&info);
                }
                _ => unreachable!(),
            }
        }
    }

    unsafe fn exit() {
        unsafe {
            let platform = System::platform_type();
            match platform {
                PlatformType::Nec98 => {
                    nec98::exit();
                }
                PlatformType::PcBios => {
                    ibm_pc::exit();
                }
                PlatformType::FmTowns => {
                    fm_towns::exit();
                }
                _ => unreachable!(),
            }
            pic::Pic::exit();
        }
    }

    fn reset_system() -> ! {
        match System::platform_type() {
            PlatformType::Nec98 => {
                nec98::reset_system();
            }
            PlatformType::PcBios => {
                ibm_pc::reset_system();
            }
            PlatformType::FmTowns => {
                fm_towns::reset_system();
            }
            _ => unreachable!(),
        }
    }

    // fn halt() -> ! {
    //     Hal::cpu().halt();
    // }

    #[inline]
    fn monotonic() -> u64 {
        pit::Pit::monotonic()
    }

    #[inline]
    fn create_timer_event(duration: Duration) -> Box<dyn PollingEvent> {
        let timeout = Self::monotonic() + pit::Pit::duration_to_ticks(duration);
        Box::new(MonotonicTimerPoller::Timeout(timeout))
    }

    fn recommended_console_mode() -> RecommendedConsoleMode {
        match System::platform_type() {
            PlatformType::FmTowns => RecommendedConsoleMode::Graphics,
            _ => RecommendedConsoleMode::None,
        }
    }
}

//! RISC-V Platform Timer implementation

use crate::arch::csr::CSR;
use crate::*;
use core::{cell::UnsafeCell, time::Duration};
use fdt::{NodeName, PropName};

static mut TIMER: UnsafeCell<PlatformTimer> = UnsafeCell::new(PlatformTimer::new());

pub struct PlatformTimer {
    monotonic_timer_value: u64,
    timer_tick: u64,
}

impl PlatformTimer {
    const TICKS_PER_SEC: u32 = 100;

    const NANOS_PER_TICK: u32 = 1_000_000_000 / Self::TICKS_PER_SEC;

    #[inline]
    const fn new() -> Self {
        Self {
            monotonic_timer_value: 0,
            timer_tick: 0,
        }
    }

    #[inline]
    unsafe fn shared<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut TIMER)).get_mut() }
    }

    pub unsafe fn init(dt: &fdt::DeviceTree) {
        unsafe {
            let shared = Self::shared();

            let cpus = dt.root().find_first_child(NodeName::CPUS).unwrap();
            let timebase_freq = cpus.get_prop_u32(PropName::TIMEBASE_FREQUENCY).unwrap();
            let timer_tick = timebase_freq / Self::TICKS_PER_SEC;

            shared.timer_tick = timer_tick as u64;

            CSR::SIE.set(1 << 5);
            Self::_set_next_timer();
        }
    }

    #[inline]
    fn _set_next_timer() {
        unsafe {
            let shared = Self::shared();
            sbi::legacy::set_timer(CSR::rdtime() + shared.timer_tick);
        }
    }

    #[inline]
    pub fn monotonic() -> u64 {
        unsafe {
            let shared = Self::shared();
            Hal::cpu().atomic_u64_load(&shared.monotonic_timer_value)
        }
    }

    #[inline]
    pub fn duration_to_ticks(duration: Duration) -> u64 {
        System::duration_to_ticks_helper32(duration, Self::NANOS_PER_TICK)
    }

    #[allow(unused)]
    pub(super) unsafe fn advance_tick() {
        unsafe {
            let shared = Self::shared();
            shared.monotonic_timer_value += 1;

            Self::_set_next_timer();
        }
    }
}

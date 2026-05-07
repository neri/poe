//! Generic Timer for AArch64

use crate::*;
use core::arch::asm;
use core::cell::UnsafeCell;
use core::time::Duration;

static mut TIMER: UnsafeCell<GenericTimer> = UnsafeCell::new(GenericTimer::new());

pub struct GenericTimer {
    monotonic_timer_value: u64,
    timer_tick: u32,
}

impl GenericTimer {
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

    pub unsafe fn init() {
        unsafe {
            let shared = Self::shared();
            shared.timer_tick = Self::counter_freq() / Self::TICKS_PER_SEC;

            Self::_set_next_timer();
        }
    }

    fn _set_next_timer() {
        unsafe {
            let shared = Self::shared();
            asm!(
                "msr cntv_tval_el0, {0}",
                "msr cntv_ctl_el0, {1}",
                in(reg) shared.timer_tick as usize,
                in(reg) 0b001usize,
            );
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

    #[inline]
    pub fn counter_freq() -> u32 {
        let cntfrq_el0: usize;
        unsafe {
            asm!("mrs {0}, cntfrq_el0", out(reg)cntfrq_el0);
        }
        cntfrq_el0 as u32
    }

    #[allow(unused)]
    pub unsafe fn advance_tick() {
        unsafe {
            let shared = Self::shared();
            shared.monotonic_timer_value += 1;

            Self::_set_next_timer();
        }
    }
}

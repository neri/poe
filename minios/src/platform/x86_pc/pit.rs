//! PIT: Programmable Interval Timer i8253/i8254

use super::pic::{Irq, IrqHandler};
use crate::*;
use core::cell::UnsafeCell;
use core::time::Duration;
use x86::isolated_io::IoPortWB;

static mut PIT: UnsafeCell<Pit> = UnsafeCell::new(Pit::new());

/// PIT: Programmable Interval Timer i8253/i8254
pub struct Pit {
    monotonic: u64,
    port_timer: IoPortWB,
    port_beep: IoPortWB,
    port_control: IoPortWB,
    timer_val: u16,
}

impl Pit {
    const NANOS_PER_TICK: u32 = 10_000_000;

    #[inline]
    const fn new() -> Self {
        Self {
            monotonic: 0,
            port_timer: IoPortWB(0),
            port_beep: IoPortWB(0),
            port_control: IoPortWB(0),
            timer_val: 0,
        }
    }

    #[inline]
    pub(super) unsafe fn init(
        port_timer: u16,
        port_beep: u16,
        port_control: u16,
        timer_val: u16,
        irq: Irq,
        irq_handler: IrqHandler,
    ) {
        unsafe {
            let shared = Self::shared();
            shared.port_timer = IoPortWB(port_timer);
            shared.port_beep = IoPortWB(port_beep);
            shared.port_control = IoPortWB(port_control);
            shared.timer_val = timer_val;

            irq.register(irq_handler).unwrap();
            shared.port_control.write(0b0011_0110u8);

            let port_timer = shared.port_timer;
            port_timer.write((timer_val & 0xff) as u8);
            port_timer.write((timer_val >> 8) as u8);
        }
    }

    #[inline]
    unsafe fn shared<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut PIT)).get_mut() }
    }

    /// Get monotonic timer value.
    pub fn monotonic() -> u64 {
        let shared = unsafe { Self::shared() };
        Hal::cpu().load_atomic_counter_u64(&shared.monotonic)
    }

    /// Convert a duration to timer ticks.
    #[inline]
    pub fn duration_to_ticks(duration: Duration) -> u64 {
        System::duration_to_ticks_helper32(duration, Self::NANOS_PER_TICK)
    }

    /// Advance monotonic timer by one tick.
    ///
    /// # SAFETY
    ///
    /// This function should only be called by the PIT interrupt handler.
    #[inline(always)]
    #[allow(dead_code)]
    pub(super) unsafe fn advance_tick(_irq: Irq) {
        let shared = unsafe { Self::shared() };
        shared.monotonic += 1;
    }
}

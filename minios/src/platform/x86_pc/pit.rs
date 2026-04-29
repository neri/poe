//! PIT: Programmable Interval Timer i8253/i8254

use super::pic::{Irq, IrqHandler};
use core::cell::UnsafeCell;
use x86::isolated_io::IoPortWB;
// use core::time::Duration;

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
    const TIMER_RES: u64 = 1;

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
        unsafe {
            let shared = Self::shared();
            let p = &shared.monotonic as *const _ as *const u32;

            // To read a 64-bit value atomically, we read the lower 32 bits, then the upper 32 bits, and check if the lower 32 bits have changed.
            // If they have, we read again. This is a common technique to read a 64-bit value on a 32-bit system without locks.
            loop {
                let lo = p.read_volatile();
                let hi = p.add(1).read_volatile();
                if lo == p.read_volatile() {
                    return ((hi as u64) << 32) | (lo as u64);
                }
            }
        }
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
        shared.monotonic += Self::TIMER_RES;
    }
}

//! PIT: Programmable Interval Timer i8253/i8254

use super::pic::Irq;
use crate::platform::x86_pc::pic::IrqHandler;
use core::cell::UnsafeCell;
use x86::isolated_io::IoPortWB;
// use core::time::Duration;

static mut PIT: UnsafeCell<Pit> = UnsafeCell::new(Pit::new());

/// PIT: Programmable Interval Timer i8253/i8254
pub struct Pit {
    monotonic: u64,
    tmr_cnt0: u16,
    beep_cnt0: u16,
    tmr_ctl: u16,
}

impl Pit {
    const TIMER_RES: u64 = 1;

    #[inline]
    const fn new() -> Self {
        Self {
            monotonic: 0,
            tmr_cnt0: 0,
            beep_cnt0: 0,
            tmr_ctl: 0,
        }
    }

    #[inline]
    pub(super) unsafe fn init(
        tmr_cnt0: u16,
        beep_cnt0: u16,
        tmr_ctl: u16,
        timer_val: u16,
        irq: Irq,
        irq_handler: IrqHandler,
    ) {
        unsafe {
            let shared = Self::shared();
            shared.tmr_cnt0 = tmr_cnt0;
            shared.beep_cnt0 = beep_cnt0;
            shared.tmr_ctl = tmr_ctl;

            irq.register(irq_handler).unwrap();
            IoPortWB(tmr_ctl).write(0b0011_0110u8);

            let cnt = IoPortWB(tmr_cnt0);
            cnt.write((timer_val & 0xff) as u8);
            cnt.write((timer_val >> 8) as u8);
        }
    }

    #[inline]
    unsafe fn shared<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut PIT)).get_mut() }
    }

    #[inline(always)]
    #[allow(dead_code)]
    pub(super) fn advance_tick(_irq: Irq) {
        let shared = unsafe { Self::shared() };
        shared.monotonic += Self::TIMER_RES;
    }
}

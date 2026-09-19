//! ARM local interrupt controller of BCM2836/BCM2837 (Raspberry Pi 2/3)
//!
//! Only the virtual timer interrupt of core 0 is supported.

use crate::arch::gic::{IRQ_CNTV, IRQ_SPURIOUS, Irq};

static mut BASE: usize = 0;

pub struct LocalIntc;

impl LocalIntc {
    pub const COMPATIBLE: &str = "brcm,bcm2836-l1-intc";

    /// Core 0 timers interrupt control
    const TIMER_CNTRL0: usize = 0x40;
    /// Core 0 IRQ source
    const IRQ_SOURCE0: usize = 0x60;

    const CNTVIRQ: u32 = 1 << 3;

    pub unsafe fn init(base: usize) {
        unsafe {
            BASE = base;
        }
    }

    pub unsafe fn enable(irq: Irq) {
        assert_eq!(irq, IRQ_CNTV, "LocalIntc: unsupported IRQ {}", irq.0);
        unsafe {
            let reg = Self::reg(Self::TIMER_CNTRL0);
            reg.write_volatile(reg.read_volatile() | Self::CNTVIRQ);
        }
    }

    /// Returns [`IRQ_SPURIOUS`] if the virtual timer interrupt is not pending.
    pub unsafe fn ack() -> Irq {
        if (unsafe { Self::reg(Self::IRQ_SOURCE0).read_volatile() } & Self::CNTVIRQ) != 0 {
            IRQ_CNTV
        } else {
            IRQ_SPURIOUS
        }
    }

    /// The timer interrupt is cleared by the timer itself.
    pub unsafe fn eoi(_irq: Irq) {}

    #[inline]
    unsafe fn reg(offset: usize) -> *mut u32 {
        unsafe { (BASE + offset) as *mut u32 }
    }
}

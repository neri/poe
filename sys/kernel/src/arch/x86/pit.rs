// Programmable Interval Timer

use super::{cpu::Cpu, pic::Irq};
use crate::task::scheduler::*;
use alloc::boxed::Box;
use bootprot::*;
use core::time::Duration;

static mut PIT: Pit = Pit::new();

pub struct Pit {
    monotonic: u64,
    tmr_cnt0: u32,
    beep_cnt0: u32,
    tmr_ctl: u32,
}

impl Pit {
    const fn new() -> Self {
        Self {
            monotonic: 0,
            tmr_cnt0: 0,
            beep_cnt0: 0,
            tmr_ctl: 0,
        }
    }

    pub(super) unsafe fn init(platform: Platform) {
        let shared = Self::shared();
        match platform {
            Platform::PcCompatible => {
                shared.tmr_cnt0 = 0x0040;
                shared.beep_cnt0 = 0x0042;
                shared.tmr_ctl = 0x0043;
                shared.init_timer(0b0011_0100, 11930);
                Irq(0).register(Self::timer_irq_handler_pc).unwrap();
            }
            Platform::Nec98 => {
                shared.tmr_cnt0 = 0x0071;
                shared.beep_cnt0 = 0x0073;
                shared.tmr_ctl = 0x0077;
                shared.init_timer(0b0011_0100, 24576);
                Irq(0).register(Self::timer_irq_handler_pc).unwrap();
            }
            Platform::FmTowns => {
                shared.tmr_cnt0 = 0x0040;
                shared.beep_cnt0 = 0x0044;
                shared.tmr_ctl = 0x0046;
                shared.init_timer(0b0011_0110, 3072);
                Irq(0).register(Self::timer_irq_handler_fmt).unwrap();
                asm!("
                    mov al, 0x81
                    out 0x60, al
                    ", out ("al") _);
            }
            _ => unreachable!(),
        }

        Timer::set_timer(&PIT);
        Cpu::enable_interrupt();
    }

    #[inline]
    fn shared<'a>() -> &'a mut Self {
        unsafe { &mut PIT }
    }

    unsafe fn init_timer(&self, cmd: u8, count: u16) {
        asm!("out dx, al", in ("edx") self.tmr_ctl, in ("al") cmd );
        asm!("
            out dx, al
            mov al, ah
            out dx, al
            ", in ("edx") self.tmr_cnt0, in ("eax") count);
    }

    /// Timer IRQ handler for PC and PC98
    fn timer_irq_handler_pc(_irq: Irq) {
        let shared = Self::shared();
        shared.monotonic += 10;
    }

    /// Timer IRQ handler for FM TOWNS
    fn timer_irq_handler_fmt(_irq: Irq) {
        let shared = Self::shared();
        shared.monotonic += 10;
        unsafe {
            asm!("
            in al, 0x60
            mov al, 0x81
            out 0x60, al
            ", out ("al") _);
        }
    }

    fn measure(&self) -> TimeSpec {
        let shared = Self::shared();
        loop {
            unsafe {
                let h: u32;
                let l: u32;
                let check: u32;
                asm!("
                    mov {1}, [{0}]
                    mov {2}, [{0}+4]
                    mov {3}, [{0}]
                ", in (reg) &shared.monotonic,
                    out (reg) l,
                    out (reg) h,
                    out (reg) check,
                );
                if l == check {
                    break ((h as u64) << 32) + (l as u64);
                }
            }
        }
    }
}

impl TimerSource for Pit {
    fn create(&self, duration: Duration) -> TimeSpec {
        self.measure() + duration.as_millis() as TimeSpec
    }

    fn until(&self, deadline: TimeSpec) -> bool {
        deadline > self.measure()
    }

    fn monotonic(&self) -> Duration {
        Duration::from_millis(self.measure())
    }
}

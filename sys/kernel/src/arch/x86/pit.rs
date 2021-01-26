// Programmable Interval Timer

use super::{cpu::Cpu, pic::Irq};
use crate::System;
use crate::{audio::*, task::scheduler::*};
use bootprot::Platform;
use core::time::Duration;

static mut PIT: Pit = Pit::new();

pub struct Pit {
    monotonic: u64,
    tmr_cnt0: u32,
    beep_cnt0: u32,
    tmr_ctl: u32,
    timer_div: usize,
}

impl Pit {
    const fn new() -> Self {
        Self {
            monotonic: 0,
            tmr_cnt0: 0,
            beep_cnt0: 0,
            tmr_ctl: 0,
            timer_div: 0,
        }
    }

    pub(super) unsafe fn init(platform: Platform) {
        let shared = Self::shared();
        match platform {
            Platform::PcCompatible => {
                shared.tmr_cnt0 = 0x0040;
                shared.beep_cnt0 = 0x0042;
                shared.tmr_ctl = 0x0043;
                shared.timer_div = 11930;
                Irq(0).register(Self::timer_irq_handler_pc).unwrap();
            }
            Platform::Nec98 => {
                shared.tmr_cnt0 = 0x0071;
                shared.beep_cnt0 = 0x0073;
                shared.tmr_ctl = 0x0077;
                shared.timer_div = 24576;
                Irq(0).register(Self::timer_irq_handler_pc).unwrap();
            }
            Platform::FmTowns => {
                shared.tmr_cnt0 = 0x0040;
                shared.beep_cnt0 = 0x0044;
                shared.tmr_ctl = 0x0046;
                shared.timer_div = 3072;
                Irq(0).register(Self::timer_irq_handler_fmt).unwrap();
                asm!("
                    mov al, 0x81
                    out 0x60, al
                    ", out ("al") _);
            }
            _ => unreachable!(),
        }

        asm!("out dx, al", in ("edx") shared.tmr_ctl, in ("al") 0b0011_0110u8);
        asm!("
            out dx, al
            mov al, ah
            out dx, al
            ", in ("edx") shared.tmr_cnt0, in ("eax") shared.timer_div);

        Timer::set_timer(&PIT);
        AudioManager::set_beep(&PIT);
        Cpu::enable_interrupt();
    }

    #[inline]
    fn shared<'a>() -> &'a mut Self {
        unsafe { &mut PIT }
    }

    /// Timer IRQ handler for IBM PC and NEC PC98
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

impl BeepDriver for Pit {
    fn make_beep(&self, freq: usize) {
        unsafe {
            Cpu::without_interrupts(|| match System::platform() {
                Platform::PcCompatible => {
                    if freq > 0 {
                        asm!("out dx, al", in ("edx") self.tmr_ctl, in ("al") 0b1011_0110u8);
                        let count = 0x0012_34DC / freq;
                        asm!("
                            out dx, al
                            mov al, ah
                            out dx, al
                            ", in ("edx") self.beep_cnt0, in ("eax") count);
                        asm!("
                            in al, 0x61
                            or al, 0x03
                            out 0x61, al
                            ", out("al") _);
                    } else {
                        asm!("
                            in al, 0x61
                            and al, 0xFC
                            out 0x61, al
                            ", out("al") _);
                    }
                }
                Platform::Nec98 => {
                    if freq > 0 {
                        asm!("out dx, al", in ("edx") self.tmr_ctl, in ("al") 0b1011_0110u8);
                        let count = 0x0025_8000 / freq;
                        asm!("
                            out dx, al
                            mov al, ah
                            out dx, al
                            ", in ("edx") self.beep_cnt0, in ("eax") count);
                        asm!("
                            mov al, 0x06
                            out 0x37, al
                            ", out("al") _);
                    } else {
                        asm!("
                            mov al, 0x07
                            out 0x37, al
                            ", out("al") _);
                    }
                }
                Platform::FmTowns => {
                    if freq > 0 {
                        asm!("out dx, al", in ("edx") self.tmr_ctl, in ("al") 0b1011_0110u8);
                        let count = 0x0004_B000 / freq;
                        asm!("
                            out dx, al
                            mov al, ah
                            out dx, al
                            ", in ("edx") self.beep_cnt0, in ("eax") count);
                        asm!("
                            in al, 0x60
                            shr al, 2
                            and al, 0x03
                            or al, 0x04
                            out 0x60, al
                            ", out("al") _);
                    } else {
                        asm!("
                            in al, 0x60
                            shr al, 2
                            and al, 0x03
                            out 0x60, al
                            ", out("al") _);
                    }
                }
                _ => (),
            })
        }
    }
}

// Programmable Interval Timer

use super::{cpu::Cpu, pic::Irq};
use crate::{audio::*, task::scheduler::*, System};
use core::arch::asm;
use core::time::Duration;
use toeboot::Platform;

static mut PIT: Pit = Pit::new();

pub struct Pit {
    monotonic: u64,
    tmr_cnt0: u32,
    beep_cnt0: u32,
    tmr_ctl: u32,
    timer_val: usize,
}

impl Pit {
    const TIMER_RES: u64 = 1;

    const fn new() -> Self {
        Self {
            monotonic: 0,
            tmr_cnt0: 0,
            beep_cnt0: 0,
            tmr_ctl: 0,
            timer_val: 0,
        }
    }

    pub(super) unsafe fn init(platform: Platform) {
        let shared = Self::shared();
        match platform {
            Platform::PcCompatible => {
                shared.tmr_cnt0 = 0x0040;
                shared.beep_cnt0 = 0x0042;
                shared.tmr_ctl = 0x0043;
                shared.timer_val = 1193;
                Irq(0).register(Self::timer_irq_handler_pc).unwrap();
            }
            Platform::Nec98 => {
                shared.tmr_cnt0 = 0x0071;
                shared.beep_cnt0 = 0x3fdb;
                shared.tmr_ctl = 0x0077;
                shared.timer_val = 2457;
                Irq(0).register(Self::timer_irq_handler_pc).unwrap();
            }
            Platform::FmTowns => {
                shared.tmr_cnt0 = 0x0040;
                shared.beep_cnt0 = 0x0044;
                shared.tmr_ctl = 0x0046;
                shared.timer_val = 307;
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
            ", in ("edx") shared.tmr_cnt0, in ("eax") shared.timer_val);

        Timer::set_timer(&PIT);
        AudioManager::set_beep_driver(&PIT);
        Cpu::enable_interrupt();
    }

    #[inline]
    fn shared<'a>() -> &'a mut Self {
        unsafe { &mut PIT }
    }

    /// Timer IRQ handler for IBM PC and NEC PC98
    fn timer_irq_handler_pc(_irq: Irq) {
        let shared = Self::shared();
        shared.monotonic += Self::TIMER_RES;
    }

    /// Timer IRQ handler for FM TOWNS
    fn timer_irq_handler_fmt(_irq: Irq) {
        let shared = Self::shared();
        shared.monotonic += Self::TIMER_RES;
        unsafe {
            asm!("
            in al, 0x60
            mov al, 0x81
            out 0x60, al
            ", out ("al") _);
        }
    }
}

impl TimerSource for Pit {
    fn measure(&self) -> TimeSpec {
        TimeSpec(self.monotonic as usize)
    }

    fn from_duration(&self, val: Duration) -> TimeSpec {
        TimeSpec((val.as_millis() as u64 / Self::TIMER_RES) as usize)
    }

    fn to_duration(&self, val: TimeSpec) -> Duration {
        Duration::from_millis(val.0 as u64 * Self::TIMER_RES)
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

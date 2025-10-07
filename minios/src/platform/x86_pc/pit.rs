//! PIT: Programmable Interval Timer i8253/i8254

use super::pic::Irq;
use crate::platform::x86_pc::pic::IrqHandler;
use core::{arch::asm, cell::UnsafeCell};
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

    /// Timer IRQ handler for IBM PC and NEC PC98
    pub(super) unsafe fn timer_irq_handler_pc(_irq: Irq) {
        let shared = unsafe { Self::shared() };
        shared.monotonic += Self::TIMER_RES;
    }

    /// Timer IRQ handler for FM TOWNS
    pub(super) unsafe fn timer_irq_handler_fmt(_irq: Irq) {
        let shared = unsafe { Self::shared() };
        shared.monotonic += Self::TIMER_RES;
        unsafe {
            asm!(
                "in al, 0x60",
                "shr al, 2",
                "or al, 0x80",
                "out 0x60, al",
                out ("al") _,
            );
        }
    }
}

// impl TimerSource for Pit {
//     fn measure(&self) -> TimeSpec {
//         TimeSpec(self.monotonic as usize)
//     }

//     fn from_duration(&self, val: Duration) -> TimeSpec {
//         TimeSpec((val.as_millis() as u64 / Self::TIMER_RES) as usize)
//     }

//     fn to_duration(&self, val: TimeSpec) -> Duration {
//         Duration::from_millis(val.0 as u64 * Self::TIMER_RES)
//     }
// }

// impl BeepDriver for Pit {
//     fn make_beep(&self, freq: usize) {
//         unsafe {
//             Cpu::without_interrupts(|| match System::platform() {
//                 Platform::PcCompatible => {
//                     if freq > 0 {
//                         asm!("out dx, al", in ("edx") self.tmr_ctl, in ("al") 0b1011_0110u8);
//                         let count = 0x0012_34DC / freq;
//                         asm!("
//                             out dx, al
//                             mov al, ah
//                             out dx, al
//                             ", in ("edx") self.beep_cnt0, in ("eax") count);
//                         asm!("
//                             in al, 0x61
//                             or al, 0x03
//                             out 0x61, al
//                             ", out("al") _);
//                     } else {
//                         asm!("
//                             in al, 0x61
//                             and al, 0xFC
//                             out 0x61, al
//                             ", out("al") _);
//                     }
//                 }
//                 Platform::Nec98 => {
//                     if freq > 0 {
//                         asm!("out dx, al", in ("edx") self.tmr_ctl, in ("al") 0b1011_0110u8);
//                         let count = 0x0025_8000 / freq;
//                         asm!("
//                             out dx, al
//                             mov al, ah
//                             out dx, al
//                             ", in ("edx") self.beep_cnt0, in ("eax") count);
//                         asm!("
//                             mov al, 0x06
//                             out 0x37, al
//                             ", out("al") _);
//                     } else {
//                         asm!("
//                             mov al, 0x07
//                             out 0x37, al
//                             ", out("al") _);
//                     }
//                 }
//                 Platform::FmTowns => {
//                     if freq > 0 {
//                         asm!("out dx, al", in ("edx") self.tmr_ctl, in ("al") 0b1011_0110u8);
//                         let count = 0x0004_B000 / freq;
//                         asm!("
//                             out dx, al
//                             mov al, ah
//                             out dx, al
//                             ", in ("edx") self.beep_cnt0, in ("eax") count);
//                         asm!("
//                             in al, 0x60
//                             shr al, 2
//                             and al, 0x03
//                             or al, 0x04
//                             out 0x60, al
//                             ", out("al") _);
//                     } else {
//                         asm!("
//                             in al, 0x60
//                             shr al, 2
//                             and al, 0x03
//                             out 0x60, al
//                             ", out("al") _);
//                     }
//                 }
//                 _ => (),
//             })
//         }
//     }
// }

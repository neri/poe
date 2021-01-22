// Programmable Interval Timer

use bootprot::*;

use super::{cpu::Cpu, pic::Irq};

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

    pub(super) unsafe fn init(platform: BootPlatform) {
        let shared = Self::shared();
        match platform {
            BootPlatform::PcCompatible => {
                shared.tmr_cnt0 = 0x0040;
                shared.beep_cnt0 = 0x0042;
                shared.tmr_ctl = 0x0043;
                shared.init_timer(0b0011_0100, 11930);
                Irq(0).register(Self::timer_irq_handler_pc).unwrap();
            }
            BootPlatform::Nec98 => {
                shared.tmr_cnt0 = 0x0071;
                shared.beep_cnt0 = 0x0073;
                shared.tmr_ctl = 0x0077;
                shared.init_timer(0b0011_0100, 24576);
                Irq(0).register(Self::timer_irq_handler_pc).unwrap();
            }
            BootPlatform::FmTowns => {
                shared.tmr_cnt0 = 0x0040;
                shared.beep_cnt0 = 0x0044;
                shared.tmr_ctl = 0x0046;
                shared.init_timer(0b0011_0110, 3072);
                asm!("
                    in al, 0x60
                    or al, 0x81
                    out 0x60, al
                    ", out ("al") _);
                Irq(0).register(Self::timer_irq_handler_fmt).unwrap();
            }
            _ => unreachable!(),
        }
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

    /// Timer IRQ handler for PC compatible and PC98
    fn timer_irq_handler_pc(_irq: Irq) {
        let shared = Self::shared();
        shared.monotonic += 1;
    }

    /// Timer IRQ handler for FM TOWNS
    fn timer_irq_handler_fmt(_irq: Irq) {
        let shared = Self::shared();
        shared.monotonic += 1;
        unsafe {
            asm!("
            in al, 0x60
            mov al, 0x81
            out 0x60, al
            ", out ("al") _);
        }
    }

    pub fn monotonic() -> u64 {
        unsafe {
            Cpu::without_interrupts(|| {
                let shared = Self::shared();
                shared.monotonic
            })
        }
    }
}

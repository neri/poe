// Real Time Clock

use super::cpu::*;
use crate::system::*;
use crate::task::scheduler::*;
use alloc::boxed::Box;
use toeboot::Platform;

static mut RTC: Rtc = Rtc::new();

pub(super) struct Rtc {
    base: u64,
    offset: u64,
    device: Option<Box<dyn RtcImpl>>,
}

impl Rtc {
    const fn new() -> Self {
        Self {
            base: 0,
            offset: 0,
            device: None,
        }
    }

    pub(super) unsafe fn init(platform: Platform) {
        let shared = Self::shared();

        match platform {
            Platform::PcCompatible => {
                shared.device = PcRtc::new();
            }
            Platform::Nec98 => {
                shared.device = N98Rtc::new();
            }
            Platform::FmTowns => {
                shared.device = FmtRtc::new();
            }
            _ => unreachable!(),
        }

        shared.base = shared.device.as_ref().unwrap().fetch_time();
        shared.offset = Timer::monotonic().as_millis() as u64;
    }

    #[inline]
    fn shared<'a>() -> &'a mut Self {
        unsafe { &mut RTC }
    }

    #[inline(never)]
    pub fn system_time() -> SystemTime {
        let shared = Self::shared();

        let millis_per_sec = 1_000;
        let diff = Timer::monotonic().as_millis() as u64 - shared.offset;
        let diff_sec = (diff / millis_per_sec) as u32;
        let secs = shared.base + diff_sec as u64;
        let nanos = (diff % millis_per_sec) as u32;

        SystemTime { secs, nanos }
    }
}

trait RtcImpl {
    unsafe fn fetch_time(&self) -> u64 {
        Cpu::without_interrupts(|| loop {
            let time1 = self.read_time();
            let time2 = self.read_time();
            if time1 == time2 {
                break time1;
            }
        })
    }

    unsafe fn read_time(&self) -> u64;
}

struct PcRtc {
    //
}

impl PcRtc {
    fn new() -> Option<Box<dyn RtcImpl>> {
        Some(Box::new(Self {}) as Box<dyn RtcImpl>)
    }
}

impl RtcImpl for PcRtc {
    unsafe fn read_time(&self) -> u64 {
        let sec = PcCmos::Seconds.read_bcd();
        let min = PcCmos::Minutes.read_bcd();
        let hour = PcCmos::Hours.read_bcd();
        (sec + min * 60 + hour * 3600) as u64
    }
}

#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
enum PcCmos {
    Seconds = 0,
    SecondsAlarm,
    Minutes,
    MinutesAlarm,
    Hours,
    HoursAlarm,
    DayOfWeek,
    DayOfMonth,
    Month,
    Year,
}

#[allow(dead_code)]
impl PcCmos {
    unsafe fn read_bcd(&self) -> usize {
        let bcd = self.read() as usize;
        (bcd & 0x0F) + (bcd / 16) * 10
    }

    unsafe fn read(&self) -> u8 {
        let mut result: u8;
        asm!("
            out 0x70, al
            in al, 0x71
            ", inout("al") *self as u8 => result);
        result
    }

    unsafe fn write(&self, data: u8) {
        asm!("
            mov al, {0}
            out 0x70, al
            mov al, {1}
            out 0x71, al
            ", in(reg_byte) *self as u8, in(reg_byte) data, out("al") _);
    }
}

struct N98Rtc {
    //
}

impl N98Rtc {
    fn new() -> Option<Box<dyn RtcImpl>> {
        Some(Box::new(Self {}) as Box<dyn RtcImpl>)
    }

    unsafe fn write_cmd(&self, cmd: u8) {
        let mut cmd = cmd;
        for _ in 0..4 {
            let data = 0x07 | ((cmd & 0x01) << 5);
            asm!("
                out 0x5F, al
                out 0x5F, al
                out 0x20, al
                xor al, 0x10
                out 0x5F, al
                out 0x5F, al
                out 0x20, al
                ", in("al") data);
            cmd >>= 1;
        }

        asm!("
            out 0x5F, al
            out 0x5F, al
            out 0x20, al
            xor al, 0x08
            out 0x5F, al
            out 0x5F, al
            out 0x20, al
            xor al, 0x08
            out 0x5F, al
            out 0x5F, al
            out 0x20, al
            ", in("al") 0x07u8);
    }

    unsafe fn read_bcd(&self) -> usize {
        let mut result: u8 = 0;
        for _ in 0..8 {
            let al: u8;
            asm!("
                out 0x5F, al
                out 0x5F, al
                in al, 0x33
                ", out("al") al);

            result >>= 1;
            if (al & 0x01) != 0 {
                result |= 0x80;
            }

            asm!("
                out 0x5F, al
                out 0x5F, al
                out 0x20, al
                xor al, 0x10
                out 0x5F, al
                out 0x5F, al
                out 0x20, al
                ", in("al") 0x17u8);
        }
        ((result & 0x0F) + (result / 16) * 10) as usize
    }
}

impl RtcImpl for N98Rtc {
    unsafe fn read_time(&self) -> u64 {
        self.write_cmd(0x03);
        self.write_cmd(0x01);
        for _ in 0..40 {
            asm!("out 0x5F, al", options(nomem));
        }
        let sec = self.read_bcd();
        let min = self.read_bcd();
        let hour = self.read_bcd();
        let _ = self.read_bcd();
        let _ = self.read_bcd();
        let _ = self.read_bcd();
        (sec + min * 60 + hour * 3600) as u64
    }
}

struct FmtRtc {
    //
}

impl FmtRtc {
    fn new() -> Option<Box<dyn RtcImpl>> {
        Some(Box::new(Self {}) as Box<dyn RtcImpl>)
    }
}

impl RtcImpl for FmtRtc {
    unsafe fn read_time(&self) -> u64 {
        // TODO:
        0
    }
}

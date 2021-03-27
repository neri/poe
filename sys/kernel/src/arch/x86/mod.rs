//

pub mod cpu;
pub mod fmtowns;
pub mod pc98;
pub mod pic;
pub mod pit;
pub mod ps2;
pub mod rtc;

use crate::system::{System, SystemTime};
use cpu::Cpu;
use megstd::drawing::IndexedColor;
use toeboot::Platform;

pub(crate) struct Arch;

impl Arch {
    pub unsafe fn init() {
        cpu::Cpu::init();

        let platform = System::platform();
        pic::Pic::init(platform);
        pit::Pit::init(platform);
        rtc::Rtc::init(platform);

        Self::init_palette(platform);
    }

    pub unsafe fn late_init() {
        let platform = System::platform();
        match platform {
            Platform::PcCompatible => {
                ps2::Ps2::init().expect("PS/2");
            }
            Platform::Nec98 => {
                pc98::Pc98::init();
            }
            Platform::FmTowns => {
                fmtowns::FmTowns::init();
            }
            _ => unreachable!(),
        }
    }

    unsafe fn init_palette(platform: Platform) {
        match platform {
            Platform::Nec98 => {
                for index in 0..0x100 {
                    let color = index as u8;
                    asm!("out 0xA8, al", in("al") color);
                    let rgb = IndexedColor(color).as_argb();
                    asm!("
                        out 0xAE, al
                        shr eax, 8
                        out 0xAA, al
                        shr eax, 8
                        out 0xAC, al
                        ", in("eax") rgb);
                }
            }
            Platform::PcCompatible => {
                for index in 0..0x100 {
                    let color = index as u8;
                    Cpu::out8(0xFD90, color);
                    let rgb = IndexedColor(color).as_argb();
                    Cpu::out8(0x3C8, color);
                    asm!("
                        rol eax, 16
                        shr al, 2
                        out dx ,al
                        rol eax, 8
                        shr al, 2
                        out dx ,al
                        rol eax, 8
                        shr al, 2
                        out dx ,al
                        ", in("eax") rgb, in("edx") 0x3C9);
                }
            }
            Platform::FmTowns => {
                for index in 0..0x100 {
                    let color = index as u8;
                    Cpu::out8(0xFD90, color);
                    let rgb = IndexedColor(color).as_argb();
                    asm!("
                        out dx, al
                        shr eax, 8
                        add dl, 4
                        out dx, al
                        shr eax, 8
                        sub dl, 2
                        out dx, al
                        ", in("eax") rgb, in("edx") 0xFD92);
                }
            }
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn system_time() -> SystemTime {
        rtc::Rtc::system_time()
    }
}

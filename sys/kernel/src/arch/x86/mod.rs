//

pub mod cpu;
pub mod fmtowns;
pub mod pc98;
pub mod pic;
pub mod pit;
pub mod ps2;
pub mod rtc;

use crate::system::*;
use toeboot::Platform;

pub(crate) struct Arch;

impl Arch {
    pub unsafe fn init() {
        cpu::Cpu::init();

        let platform = System::platform();
        pic::Pic::init(platform);
        pit::Pit::init(platform);
        rtc::Rtc::init(platform);
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

    #[inline]
    pub fn system_time() -> SystemTime {
        rtc::Rtc::system_time()
    }
}

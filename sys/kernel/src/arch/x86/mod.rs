//

pub mod cpu;
pub mod fmtowns;
pub mod pc98;
pub mod pic;
pub mod pit;
pub mod ps2;

use crate::system::*;
use toeboot::Platform;

pub(crate) struct Arch;

impl Arch {
    pub unsafe fn init() {
        cpu::Cpu::init();

        let platform = System::platform();
        pic::Pic::init(platform);
        pit::Pit::init(platform);

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
            _ => (),
        }
    }
}

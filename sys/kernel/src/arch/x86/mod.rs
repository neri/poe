pub mod cpu;
pub mod pic;
pub mod pit;

use crate::system::*;

pub(crate) struct Arch;

impl Arch {
    pub unsafe fn init() {
        cpu::Cpu::init();

        pic::Pic::init(System::platform());
        pit::Pit::init(System::platform());
    }
}

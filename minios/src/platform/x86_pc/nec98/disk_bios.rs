//! PC-98 Disk BIOS Driver

use x86::gpr::Flags;
use x86::prot::Selector;

use super::bios::INT1B;
use super::*;
use crate::arch::vm86::Vm86Context;

pub(super) struct DiskBios {
    //
}

impl DiskBios {
    #[inline(never)]
    pub unsafe fn init() {
        // let info = System::boot_info();
        // unsafe {
        //     let mut regs = Vm86Context::default();
        //
        // }
        // todo!()
    }
}

#[allow(dead_code)]
fn print_disk_type(_drive: u8, _regs: &mut Vm86Context) {
    //
}

#[allow(dead_code)]
struct INT1BDevice {
    drive_spec: BiosDriveSpec,
    media_info: MediaInfo,
}

impl INT1BDevice {
    // pub fn new() -> Self {
    //     Self {}
    // }
}

#[allow(dead_code)]
impl BlockDevice for INT1BDevice {
    fn reset(&mut self) -> Result<(), BlockIoError> {
        unimplemented!()
    }

    fn read(&mut self, _block: LBA, _buf: &mut [u8]) -> Result<(), BlockIoError> {
        unimplemented!()
    }

    fn write(&mut self, _block: LBA, _buf: &[u8]) -> Result<(), BlockIoError> {
        unimplemented!()
    }

    fn media_info(&self) -> &MediaInfo {
        &self.media_info
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum INT1BErrorCode {
    Success = 0,
    UnknownError,
}

#[allow(dead_code)]
impl INT1BErrorCode {
    #[inline]
    pub fn from_u8(code: u8) -> Self {
        match code {
            0 => Self::Success,
            _ => Self::UnknownError,
        }
    }
}

//! Disk Bios Driver

use super::{bios, *};
use arch::{cpu::X86StackContext, vm86::VM86};
use x86::{gpr::Flags, prot::Selector};

pub(super) struct DiskBios {
    //
}

impl DiskBios {
    #[inline(never)]
    pub unsafe fn init() {
        // let info = Environment::boot_info();
        // unsafe {
        //     let mut regs = X86StackContext::default();

        //     println!("boot drive: {:02x}", info.bios_boot_drive.0);
        //     print_disk_type(info.bios_boot_drive.0, &mut regs);

        //     for i in 0..2 {
        //         print_disk_type(i, &mut regs);
        //     }

        //     let p = 0x475 as *const u8;
        //     let n_hdds = p.read_volatile();
        //     println!("number of hard drive: {:02x}", n_hdds);
        //     if n_hdds > 0 {
        //         for i in 0..n_hdds {
        //             print_disk_type(0x80 + i, &mut regs);
        //         }
        //     } else {
        //         print_disk_type(0x80, &mut regs);
        //     }
        //     println!("");
        // }
    }
}

#[allow(dead_code)]
fn print_disk_type(drive: u8, regs: &mut X86StackContext) {
    let drive_type: u8;

    regs.eax = 0x15ff;
    regs.ecx = 0xffff;
    regs.edx = drive as u32;
    unsafe {
        VM86::call_bios(bios::INT13, regs);
    }
    if regs.eflags.contains(Flags::CF) {
        println!("drive {:02x}: error {:02x}", drive, regs.ah());
        return;
    } else {
        drive_type = regs.ah();
    }

    regs.eax = 0x0800;
    regs.edx = drive as u32;
    unsafe { regs.set_vmes(Selector::NULL) };
    regs.edi = 0;
    unsafe {
        VM86::call_bios(bios::INT13, regs);
    }
    if regs.eflags.contains(Flags::CF) {
        println!("drive {:02x}: {:02x}", drive, drive_type);
        return;
    }

    let chs = [regs.cl(), regs.ch(), regs.dh()];

    println!(
        "drive {:02x}: {:02x} {:02x} {:02x?} {:02x}",
        drive,
        drive_type,
        regs.ah(),
        chs,
        regs.dl(),
    );
}

#[allow(dead_code)]
struct Int13Device {
    drive_spec: BiosDriveSpec,
    media_info: MediaInfo,
}

impl Int13Device {
    // pub fn new() -> Self {
    //     Self {}
    // }
}

#[allow(dead_code)]
impl BlockDevice for Int13Device {
    fn reset(&mut self) -> Result<(), BlockIoError> {
        unimplemented!()
    }

    fn read(&mut self, _block: LBA, _buf: &mut [u8]) -> Result<(), BlockIoError> {
        unimplemented!()
    }

    fn write(&mut self, _block: LBA, _buf: &[u8]) -> Result<(), BlockIoError> {
        unimplemented!()
    }

    fn media_info(&self) -> MediaInfo {
        unimplemented!()
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Int13ErrorCode {
    Success = 0,
    InvalidParameter = 1,
    AddressMarkNotFound = 2,
    WriteProtected = 3,
    SectorNotFound = 4,
    ResetFailed = 5,
    DiskChanged = 6,
    DmaOverrun = 8,
    DataBoundaryError = 9,
    BadSectorDetected = 10,
    BadTrackDetected = 11,
    InvalidMedia = 12,
    SeekFailed = 0x40,
    TimedOut = 0x80,
    DriveNotReady = 0xaa,
    UnknownError,
}

#[allow(dead_code)]
impl Int13ErrorCode {
    #[inline]
    pub fn from_u8(code: u8) -> Self {
        match code {
            0 => Self::Success,
            1 => Self::InvalidParameter,
            2 => Self::AddressMarkNotFound,
            3 => Self::WriteProtected,
            4 => Self::SectorNotFound,
            5 => Self::ResetFailed,
            6 => Self::DiskChanged,
            8 => Self::DmaOverrun,
            9 => Self::DataBoundaryError,
            10 => Self::BadSectorDetected,
            11 => Self::BadTrackDetected,
            12 => Self::InvalidMedia,
            0x40 => Self::SeekFailed,
            0x80 => Self::TimedOut,
            0xaa => Self::DriveNotReady,
            _ => Self::UnknownError,
        }
    }
}

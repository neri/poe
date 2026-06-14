//! FM TOWNS Disk BIOS Driver
//!
//! NOTE: Because the FM TOWNS has limited BIOS functionality in a bare-metal environment, the current implementation is also limited.
//!

use core::cell::UnsafeCell;

use fatfs::bpb::BootSector;
use x86::real::Far16Ptr;

use super::*;
use crate::arch::lomem::{LoMemoryManager, ManagedLowMemory};
use crate::arch::vm86::{VM86, Vm86Context};
use crate::io::fs::media::*;

static mut SHARED: UnsafeCell<DiskBios> = UnsafeCell::new(DiskBios::new());

pub(super) struct DiskBios {
    boot_drive: FmBiosDriveSpec,
    devices: Vec<Int93Device>,
    io_buffer: Option<ManagedLowMemory>,
}

impl DiskBios {
    #[inline]
    const fn new() -> Self {
        Self {
            boot_drive: FmBiosDriveSpec(0),
            devices: Vec::new(),
            io_buffer: None,
        }
    }

    #[inline]
    unsafe fn shared() -> &'static mut Self {
        unsafe { (&mut *(&raw mut SHARED)).get_mut() }
    }

    #[inline(never)]
    pub unsafe fn init() {
        let info = System::boot_info();
        unsafe {
            let shared = Self::shared();

            shared.boot_drive = FmBiosDriveSpec(info.bios_boot_drive.0);
            shared.io_buffer = Some(LoMemoryManager::alloc_page());

            let mut devices = Vec::new();
            for i in (0x31dc..=0x321a).step_by(4) {
                let dev_type = IoPortRB(i).read();
                let unit_number = IoPortRB(i + 2).read();
                match dev_type {
                    0 => {
                        // floppy
                        if let Ok(device) = Int93Device::identify(FmBiosDriveSpec::new(
                            FmBiosDeviceType::FLOPPY,
                            unit_number,
                        )) {
                            devices.push(device);
                        }
                    }
                    2 => {
                        // TODO: scsi hard drive
                    }
                    5 => {
                        // TODO: rom drive
                    }
                    _ => {
                        // unknown device type, ignore
                    }
                }
            }
            for i in 0..1 {
                let drive_spec = FmBiosDriveSpec::new(FmBiosDeviceType::CDROM, i);
                if let Ok(device) = Int93Device::identify(drive_spec) {
                    devices.push(device);
                }
            }

            shared.devices = devices;
        }
    }

    /// The entry point of the Disk BIOS
    pub const DISK_BIOS_ENTRY: Far16Ptr = Far16Ptr::from_u32(0xfffb_0014);

    /// Invokes the Disk BIOS
    #[inline]
    unsafe fn call_bios(regs: &mut Vm86Context) {
        unsafe {
            VM86::call_far(Self::DISK_BIOS_ENTRY, regs);
        }
    }
}

pub struct Int93Device {
    drive_spec: FmBiosDriveSpec,
    geometry: Geometry,
    media_info: MediaInfo,
}

impl Int93Device {
    fn identify(drive_spec: FmBiosDriveSpec) -> Result<Self, BlockIoError> {
        let mut device = Self {
            drive_spec: drive_spec,
            geometry: Geometry::EMPTY,
            media_info: MediaInfo::EMPTY,
        };

        let mut regs = Vm86Context::default();
        if device.drive_spec.is_floppy() {
            // let _ = device.reset_media(&mut regs);
            Ok(device)
        } else {
            device.reset_media(&mut regs).map(|_| device)
        }
    }

    fn reset_media(&mut self, regs: &mut Vm86Context) -> Result<(), BlockIoError> {
        unsafe {
            let disk_bios = DiskBios::shared();
            let io_buffer = disk_bios.io_buffer.as_ref().unwrap();

            match self.drive_spec.device_type() {
                FmBiosDeviceType::FLOPPY => {
                    io_buffer.clear();

                    regs.eax.set_hl(0x05, self.drive_spec.0);
                    regs.ecx.set_zero();
                    regs.edx.set_hl(0, 1);
                    regs.ebx.set_d(1);
                    regs.set_vmds(io_buffer.sel());
                    regs.edi.set_zero();
                    DiskBios::call_bios(regs);
                    if regs.ah() != 0 {
                        return Err(BlockIoError::DeviceError);
                    }

                    let Some(bs) = BootSector::from_bytes(io_buffer.as_slice()) else {
                        return Err(BlockIoError::DeviceError);
                    };
                    let bpb = bs.bpb();
                    let Some(total_sectors) = bpb.total_sectors() else {
                        return Err(BlockIoError::DeviceError);
                    };
                    let s = bpb.sectors_per_track as u8;
                    let h = bpb.n_heads as u8;
                    let c = (total_sectors / (s as u32 * h as u32)) as u16;
                    self.geometry = Geometry { c, h, s };
                    self.media_info.block_count = LBA(total_sectors as u64);
                    self.media_info.block_size = bpb.bytes_per_sector as u32;

                    Ok(())
                }
                FmBiosDeviceType::HARD_DISK => {
                    // TODO:
                    Err(BlockIoError::NoMedia)
                }
                FmBiosDeviceType::CDROM => {
                    if self.drive_spec.unit_number() != 0 {
                        return Err(BlockIoError::NoMedia);
                    }
                    self.geometry = Geometry::EMPTY;
                    self.media_info.block_count = LBA(0);
                    self.media_info.block_size = 2048;
                    Ok(())
                }
                _ => Err(BlockIoError::NoMedia),
            }
        }
    }

    fn handle_error(&mut self, regs: &mut Vm86Context) -> Result<(), BlockIoError> {
        if regs.ah() == 0 {
            return Ok(());
        }
        let cx = regs.cx();
        if cx & 0x0001 != 0 {
            return Err(BlockIoError::NoMedia);
        }
        Err(BlockIoError::DeviceError)
    }

    /// Perform a read/write operation using bios.
    unsafe fn transfer(
        &mut self,
        function: TransferFunction,
        lba: LBA,
    ) -> Result<(), BlockIoError> {
        unsafe {
            let block_size = self.media_info.block_size as usize;
            if block_size == 0 {
                return Err(BlockIoError::NoMedia);
            }

            let mut bytes_left = function.byte_count();
            if bytes_left == 0 || bytes_left % block_size != 0 {
                return Err(BlockIoError::BadBufferSize);
            }
            let mut target_addr = function.target_addr();

            let disk_bios = DiskBios::shared();
            let io_buffer = disk_bios.io_buffer.as_ref().unwrap();

            let mut regs = Vm86Context::default();
            let mut lba = lba;

            if self.drive_spec.is_floppy() {
                if self.geometry.is_empty() {
                    return Err(BlockIoError::InvalidParameter);
                }

                while bytes_left > 0 {
                    if lba >= self.media_info.block_count {
                        return Err(BlockIoError::InvalidParameter);
                    }

                    let chs = self
                        .geometry
                        .convert(lba)
                        .ok_or(BlockIoError::InvalidParameter)?;

                    match function {
                        TransferFunction::Read(_) => {
                            io_buffer.clear();
                        }
                        TransferFunction::Write(_) => {
                            io_buffer
                                .as_slice()
                                .as_mut_ptr()
                                .copy_from_nonoverlapping(target_addr as *const u8, block_size);
                        }
                    }

                    regs.eax.set_hl(function.func_no(), self.drive_spec.0);
                    regs.ecx.set_d(chs.c as u32);
                    regs.edx.set_hl(chs.h, chs.s);
                    regs.ebx.set_d(1);
                    regs.set_vmds(io_buffer.sel());
                    regs.edi.set_zero();
                    DiskBios::call_bios(&mut regs);
                    self.handle_error(&mut regs)?;

                    match function {
                        TransferFunction::Read(_) => {
                            (target_addr as *mut u8).copy_from_nonoverlapping(
                                io_buffer.as_slice().as_ptr(),
                                block_size,
                            );
                        }
                        TransferFunction::Write(_) => {
                            // No need to copy back for write operation
                        }
                    }

                    lba += 1;
                    target_addr += block_size;
                    bytes_left -= block_size;
                }

                Ok(())
            } else {
                while bytes_left > 0 {
                    match function {
                        TransferFunction::Read(_) => {
                            io_buffer.clear();
                        }
                        TransferFunction::Write(_) => {
                            io_buffer
                                .as_slice()
                                .as_mut_ptr()
                                .copy_from_nonoverlapping(target_addr as *const u8, block_size);
                        }
                    }

                    regs.eax.set_hl(function.func_no(), self.drive_spec.0);
                    regs.ecx.set_d(lba.0 as u32);
                    regs.edx.set_d((lba.0 as u32) >> 16);
                    regs.ebx.set_d(1);
                    regs.set_vmds(io_buffer.sel());
                    regs.ebp.set_zero();
                    DiskBios::call_bios(&mut regs);
                    self.handle_error(&mut regs)?;

                    match function {
                        TransferFunction::Read(_) => {
                            (target_addr as *mut u8).copy_from_nonoverlapping(
                                io_buffer.as_slice().as_ptr(),
                                block_size,
                            );
                        }
                        TransferFunction::Write(_) => {
                            // No need to copy back for write operation
                        }
                    }

                    lba += 1;
                    target_addr += block_size;
                    bytes_left -= block_size;
                }

                Ok(())
            }
        }
    }
}

#[allow(dead_code)]
impl BlockDevice for Int93Device {
    fn reset(&mut self) -> Result<(), BlockIoError> {
        let mut regs = Vm86Context::default();
        self.reset_media(&mut regs)
    }

    fn read(&mut self, lba: LBA, buffer: &mut [u8]) -> Result<(), BlockIoError> {
        unsafe { self.transfer(TransferFunction::Read(buffer), lba) }
    }

    fn write(&mut self, _lba: LBA, _buffer: &[u8]) -> Result<(), BlockIoError> {
        Err(BlockIoError::WriteProtected)
    }

    fn media_info(&mut self) -> &MediaInfo {
        &self.media_info
    }
}

/// FM TOWNS BIOS Drive Specifier
#[derive(Debug, Clone, Copy)]
pub struct FmBiosDriveSpec(pub u8);

#[allow(unused)]
impl FmBiosDriveSpec {
    #[inline]
    pub fn new(device_type: FmBiosDeviceType, unit_number: u8) -> Self {
        Self(device_type.0 | (unit_number & 0x0f))
    }

    #[inline]
    pub fn device_type(&self) -> FmBiosDeviceType {
        FmBiosDeviceType(self.0 & 0xf0)
    }

    #[inline]
    pub fn unit_number(&self) -> u8 {
        self.0 & 0x0f
    }

    #[inline]
    pub fn is_floppy(&self) -> bool {
        self.device_type() == FmBiosDeviceType::FLOPPY
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FmBiosDeviceType(pub u8);

#[allow(unused)]
impl FmBiosDeviceType {
    pub const FLOPPY: Self = Self(0x20);

    pub const HARD_DISK: Self = Self(0xb0);

    pub const CDROM: Self = Self(0xc0);
}

#[allow(unused)]
#[derive(Debug)]
pub enum TransferFunction<'a> {
    Read(&'a mut [u8]),
    Write(&'a [u8]),
    // Verify(&'a [u8]),
}

impl TransferFunction<'_> {
    #[inline]
    pub fn func_no(&self) -> u8 {
        match self {
            Self::Read(_) => 0x05,
            Self::Write(_) => 0x06,
        }
    }

    #[inline]
    pub fn byte_count(&self) -> usize {
        match self {
            Self::Read(buf) => buf.len(),
            Self::Write(buf) => buf.len(),
        }
    }

    #[inline]
    pub fn target_addr(&self) -> usize {
        match self {
            Self::Read(target) => target.as_ptr() as usize,
            Self::Write(target) => target.as_ptr() as usize,
        }
    }
}

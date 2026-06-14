//! PC-98 Disk BIOS Driver

use core::cell::UnsafeCell;
use core::default;

use super::bios::INT1B;
use super::*;
use crate::arch::lomem::{LoMemoryManager, ManagedLowMemory};
use crate::arch::vm86::Vm86Context;
use crate::io::fs::media::*;

static mut SHARED: UnsafeCell<DiskBios> = UnsafeCell::new(DiskBios::new());

pub(super) struct DiskBios {
    boot_drive: DaUa,
    devices: Vec<Int1BDevice>,
    io_buffer: Option<ManagedLowMemory>,
}

impl DiskBios {
    #[inline]
    const fn new() -> Self {
        Self {
            boot_drive: DaUa(0),
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

            shared.boot_drive = DaUa(info.bios_boot_drive.0).representative();
            shared.io_buffer = Some(LoMemoryManager::alloc_page());

            let disk_equip = (0x55c as *const u16).read_volatile();

            let mut devices = Vec::new();
            for i in 0..2 {
                let daua = DaUa(i).another_device(Da::FLOPPY);
                devices.push(Int1BDevice::identify(daua).unwrap());
            }
            for i in 0..4 {
                if disk_equip & (1 << i) == 0 {
                    continue;
                }
                let daua = DaUa(i).another_device(Da::HD_RBA);
                let Ok(device) = Int1BDevice::identify(daua) else {
                    continue;
                };
                devices.push(device);
            }

            shared.devices = devices;
        }
    }
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
            Self::Read(_) => 0x56,
            Self::Write(_) => 0x55,
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

/// DA/UA: PC-98 Drive specifier used in INT1Bh.
///
/// It encodes both the unit address and the device address of a drive.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaUa(pub u8);

#[allow(unused)]
impl DaUa {
    /// Returns whether the drive is a floppy drive or not.
    #[inline]
    pub const fn is_floppy(&self) -> bool {
        (self.0 & 0x10) != 0
    }

    /// Returns unit address of the drive.
    #[inline]
    pub const fn ua(&self) -> u8 {
        self.0 & 0x0f
    }

    /// Returns device address of the drive.
    #[inline]
    pub const fn da(&self) -> Da {
        Da(self.0 & 0xf0)
    }

    /// Returns another DaUa with the same unit address but different device address.
    #[inline]
    pub const fn another_device(&self, da: Da) -> Self {
        Self(self.ua() | da.0)
    }

    /// Returns a representative value for the drive
    #[inline]
    pub fn representative(&self) -> Self {
        if self.is_floppy() {
            self.another_device(Da::FLOPPY)
        } else {
            Self(self.0 | 0x80)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Da(pub u8);

#[allow(unused)]
impl Da {
    pub const FLOPPY: Self = Self::FD_2HD;

    /// SASI or IDE hard disk Relative Addressing mode
    pub const HD_RBA: Self = Self(0x00);

    /// 2DD mode on 1mb mode
    pub const FD_2DD_ON_1MB: Self = Self(0x10);

    /// SCSI hard disk Relative Addressing mode
    pub const SCSI_RBA: Self = Self(0x20);

    /// 1.44MB floppy
    pub const FD_1_4M: Self = Self(0x30);

    /// 320KB floppy
    pub const FD_320KB: Self = Self(0x50);

    /// 2DD mode on 640kb mode
    pub const FD_2DD_ON_640KB: Self = Self(0x70);

    /// SASI or IDE hard disk CHS mode
    pub const HD_CHS: Self = Self(0x80);

    /// 2HD mode on 1mb mode (default 2HD)
    pub const FD_2HD: Self = Self(0x90);

    /// SCSI hard disk CHS mode
    pub const SCSI_CHS: Self = Self(0xa0);

    /// 2HD mode on 640kb mode
    pub const FD_2HD_ON_640KB: Self = Self(0xf0);
}

pub struct Int1BDevice {
    #[allow(unused)]
    representative_daua: DaUa,
    daua: DaUa,
    geometry: BiosGeometry,
    media_info: MediaInfo,
}

impl Int1BDevice {
    unsafe fn identify(drive_spec: DaUa) -> Result<Self, BlockIoError> {
        let daua = drive_spec.representative();
        let mut device = Self {
            representative_daua: daua,
            daua,
            geometry: BiosGeometry::default(),
            media_info: MediaInfo::EMPTY,
        };

        let mut regs = Vm86Context::default();
        if device.daua.is_floppy() {
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

            if self.daua.is_floppy() {
                // Recalibrate
                regs.eax.set_hl(0x07, self.daua.0);
                INT1B.call(regs);
                if regs.eflags().is_c() {
                    return Err(Self::convert_status_code(regs.ah()));
                }

                // Attempt to read as 1.44MB floppy
                let current_daua = self.daua.another_device(Da::FD_1_4M);
                regs.eax.set_hl(0x76, current_daua.0);
                regs.ecx.set_d(0x0200);
                regs.edx.set_d(0x0001);
                regs.ebx.set_d(0x0200);
                regs.set_vmes(io_buffer.sel());
                regs.ebp.set_zero();
                INT1B.call(regs);
                if regs.eflags().is_nc() {
                    self.daua = current_daua;
                    let chrn = CHRN::new(80, 2, 18, 2);
                    self.geometry = BiosGeometry::CHRN(chrn);
                    self.media_info = chrn.media_info_template();
                    return Ok(());
                }

                // Attempt to read as 2HD
                let current_daua = self.daua.another_device(Da::FD_2HD);
                regs.eax.set_hl(0x7a, current_daua.0);
                regs.ecx.set_zero();
                regs.edx.set_zero();
                INT1B.call(regs);
                if regs.eflags().is_nc() {
                    self.daua = current_daua;
                    let n = regs.ch();
                    let chrn = match n {
                        2 => CHRN::new(80, 2, 15, n),
                        3 => CHRN::new(77, 2, 8, n),
                        _ => CHRN::EMPTY,
                    };
                    if !chrn.is_empty() {
                        regs.eax.set_hl(0x76, self.daua.0);
                        regs.ecx.set_b(chrn.c - 1);
                        regs.edx.set_hl(chrn.h - 1, chrn.r);
                        regs.ebx.set_d(128 << n as usize);
                        regs.set_vmes(io_buffer.sel());
                        regs.ebp.set_zero();
                        INT1B.call(regs);
                        if regs.eflags().is_nc() {
                            self.geometry = BiosGeometry::CHRN(chrn);
                            self.media_info = chrn.media_info_template();
                            return Ok(());
                        }
                    }
                }
            } else {
                regs.eax.set_hl(0x84, self.daua.0);
                INT1B.call(regs);
                if regs.eflags().is_nc() {
                    let chs = Geometry::new(regs.cx(), regs.dh(), regs.dl());
                    self.geometry = BiosGeometry::CHS(chs);
                    self.media_info.block_size = regs.bx() as u32;
                    self.media_info.block_count = LBA(chs.total_sectors() as u64);
                    return Ok(());
                }
            }

            self.geometry = BiosGeometry::default();
            self.media_info = MediaInfo::EMPTY;
            Err(BlockIoError::NoMedia)
        }
    }

    fn convert_status_code(code: u8) -> BlockIoError {
        let code = Int1BStatusCode::from_u8(code);
        match code {
            Int1BStatusCode::Success => BlockIoError::DeviceError,
            Int1BStatusCode::WriteProtected => BlockIoError::WriteProtected,
            Int1BStatusCode::DMABoundary => BlockIoError::DeviceError,
            Int1BStatusCode::EndOfCylinder => BlockIoError::DeviceError,
            Int1BStatusCode::EquipmentCheck => BlockIoError::DeviceError,
            Int1BStatusCode::Overrun => BlockIoError::DeviceError,
            Int1BStatusCode::NotReady => BlockIoError::NoMedia,
            Int1BStatusCode::NotWritable => BlockIoError::WriteProtected,
            Int1BStatusCode::Error => BlockIoError::DeviceError,
            Int1BStatusCode::TimeOut => BlockIoError::DeviceError,
            Int1BStatusCode::DataErrorId => BlockIoError::DeviceError,
            Int1BStatusCode::DataError => BlockIoError::DeviceError,
            Int1BStatusCode::NoData => BlockIoError::DeviceError,
            Int1BStatusCode::BadCylinder => BlockIoError::DeviceError,
            Int1BStatusCode::MissingAddressMarkId => BlockIoError::DeviceError,
            Int1BStatusCode::MissingAddressMarkData => BlockIoError::DeviceError,
            Int1BStatusCode::UnknownError => BlockIoError::DeviceError,
        }
    }

    fn handle_error(&mut self, regs: &mut Vm86Context) -> Result<(), BlockIoError> {
        if regs.eflags().is_nc() {
            return Ok(());
        }
        let error = Self::convert_status_code(regs.ah());
        match error {
            BlockIoError::NoMedia => {
                self.geometry = BiosGeometry::default();
                self.media_info = MediaInfo::EMPTY;
            }
            _ => {}
        }
        Err(error)
    }

    /// Perform a read/write operation using INT1Bh.
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
            let limit_secs = io_buffer.limit().as_u32() as usize / block_size;

            let mut regs = Vm86Context::default();
            let mut lba = lba;

            if self.daua.is_floppy() {
                let geometry = match self.geometry {
                    BiosGeometry::CHRN(chrn) => chrn,
                    _ => return Err(BlockIoError::InvalidParameter),
                };
                if geometry.is_empty() {
                    return Err(BlockIoError::InvalidParameter);
                }

                while bytes_left > 0 {
                    if lba >= self.media_info.block_count {
                        return Err(BlockIoError::InvalidParameter);
                    }

                    let chs = geometry
                        .to_chs()
                        .convert(lba)
                        .ok_or(BlockIoError::InvalidParameter)?;

                    let transfer_secs = (bytes_left / block_size)
                        .min(geometry.r as usize - (chs.s as usize) + 1)
                        .clamp(1, limit_secs) as u8;
                    let transfer_size = block_size * transfer_secs as usize;

                    match function {
                        TransferFunction::Read(_) => {
                            io_buffer.clear();
                        }
                        TransferFunction::Write(_) => {
                            io_buffer
                                .as_slice()
                                .as_mut_ptr()
                                .copy_from_nonoverlapping(target_addr as *const u8, transfer_size);
                        }
                    }

                    regs.eax.set_hl(function.func_no(), self.daua.0);
                    regs.ecx.set_hl(geometry.n, chs.c as u8);
                    regs.edx.set_hl(chs.h, chs.s);
                    regs.ebx.set_d(transfer_size as u32);
                    regs.set_vmes(io_buffer.sel());
                    regs.ebp.set_zero();
                    INT1B.call(&mut regs);
                    self.handle_error(&mut regs)?;

                    match function {
                        TransferFunction::Read(_) => {
                            (target_addr as *mut u8).copy_from_nonoverlapping(
                                io_buffer.as_slice().as_ptr(),
                                transfer_size,
                            );
                        }
                        TransferFunction::Write(_) => {
                            // No need to copy back for write operation
                        }
                    }

                    lba += transfer_secs as u64;
                    target_addr += transfer_size;
                    bytes_left -= transfer_size;
                }

                Ok(())
            } else {
                while bytes_left > 0 {
                    if lba >= self.media_info.block_count {
                        return Err(BlockIoError::InvalidParameter);
                    }

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

                    regs.eax.set_hl(function.func_no(), self.daua.0 & 0x7f);
                    regs.ecx.set_d(lba.0 as u32);
                    regs.edx.set_d((lba.0 as u32) >> 16);
                    regs.ebx.set_d(block_size as u32);
                    regs.set_vmes(io_buffer.sel());
                    regs.ebp.set_zero();
                    INT1B.call(&mut regs);
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
impl BlockDevice for Int1BDevice {
    fn reset(&mut self) -> Result<(), BlockIoError> {
        let mut regs = Vm86Context::default();
        self.reset_media(&mut regs)
    }

    fn read(&mut self, lba: LBA, buffer: &mut [u8]) -> Result<(), BlockIoError> {
        unsafe { self.transfer(TransferFunction::Read(buffer), lba) }
    }

    fn write(&mut self, lba: LBA, buffer: &[u8]) -> Result<(), BlockIoError> {
        unsafe { self.transfer(TransferFunction::Write(buffer), lba) }
    }

    fn media_info(&mut self) -> &MediaInfo {
        &self.media_info
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Int1BStatusCode {
    Success = 0x00,
    WriteProtected = 0x10,
    DMABoundary = 0x20,
    EndOfCylinder = 0x30,
    EquipmentCheck = 0x40,
    Overrun = 0x50,
    NotReady = 0x60,
    NotWritable = 0x70,
    Error = 0x80,
    TimeOut = 0x90,
    DataErrorId = 0xa0,
    DataError = 0xb0,
    NoData = 0xc0,
    BadCylinder = 0xd0,
    MissingAddressMarkId = 0xe0,
    MissingAddressMarkData = 0xf0,
    UnknownError,
}

#[allow(dead_code)]
impl Int1BStatusCode {
    #[inline]
    pub fn from_u8(code: u8) -> Self {
        match code {
            0x00 => Self::Success,
            0x10 => Self::WriteProtected,
            0x20 => Self::DMABoundary,
            0x30 => Self::EndOfCylinder,
            0x40 => Self::EquipmentCheck,
            0x50 => Self::Overrun,
            0x60 => Self::NotReady,
            0x70 => Self::NotWritable,
            0x80 => Self::Error,
            0x90 => Self::TimeOut,
            0xa0 => Self::DataErrorId,
            0xb0 => Self::DataError,
            0xc0 => Self::NoData,
            0xd0 => Self::BadCylinder,
            0xe0 => Self::MissingAddressMarkId,
            0xf0 => Self::MissingAddressMarkData,
            _ => Self::UnknownError,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum BiosGeometry {
    CHRN(CHRN),
    CHS(Geometry),
}

impl default::Default for BiosGeometry {
    #[inline]
    fn default() -> Self {
        Self::CHRN(CHRN::EMPTY)
    }
}

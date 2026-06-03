//! PC compatible Disk BIOS Driver

use core::cell::UnsafeCell;

use x86::prot::Selector;
use x86::real::Far16Ptr;

use super::bios::INT13;
use super::*;
use crate::arch::lomem::ManagedLowMemory;
use crate::arch::vm86::Vm86Context;

static mut SHARED: UnsafeCell<DiskBios> = UnsafeCell::new(DiskBios::new());

pub(super) struct DiskBios {
    boot_drive: BiosDriveSpec,
    devices: Vec<Int13Device>,
    io_buffer: Option<ManagedLowMemory>,
    packet_buffer: Option<ManagedLowMemory>,
}

impl DiskBios {
    #[inline]
    const fn new() -> Self {
        Self {
            boot_drive: BiosDriveSpec(0),
            devices: Vec::new(),
            io_buffer: None,
            packet_buffer: None,
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

            shared.boot_drive = info.bios_boot_drive;
            shared.io_buffer = Some(LoMemoryManager::alloc_page());
            shared.packet_buffer = Some(LoMemoryManager::alloc_page());

            let mut devices = Vec::new();
            for i in 0..2 {
                let drive_spec = BiosDriveSpec(i);
                devices.push(Int13Device::identity(drive_spec).unwrap());
            }
            for i in 0x80..0xff {
                let drive_spec = BiosDriveSpec(i);
                let Ok(device) = Int13Device::identity(drive_spec) else {
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
            Self::Read(_) => 0x02,
            Self::Write(_) => 0x03,
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

#[allow(dead_code)]
struct Int13Device {
    drive_spec: BiosDriveSpec,
    geometry: Geometry,
    is_lba_supported: bool,
    media_info: MediaInfo,
}

impl Int13Device {
    /// Returns whether the drive is a floppy drive or not.
    #[inline]
    pub const fn is_floppy(drive_spec: BiosDriveSpec) -> bool {
        drive_spec.0 < 0x80
    }

    unsafe fn identity(drive_spec: BiosDriveSpec) -> Result<Self, BlockIoError> {
        let mut device = Self {
            drive_spec,
            geometry: Geometry::EMPTY,
            is_lba_supported: false,
            media_info: MediaInfo::EMPTY,
        };

        let mut regs = Vm86Context::default();
        if Self::is_floppy(drive_spec) {
            let _ = device.reset_media(&mut regs);
            Ok(device)
        } else {
            device.reset_media(&mut regs).map(|_| device)
        }
    }

    fn reset_media(&mut self, regs: &mut Vm86Context) -> Result<(), BlockIoError> {
        unsafe {
            let disk_bios = DiskBios::shared();
            let packet_buffer = disk_bios.packet_buffer.as_ref().unwrap();
            packet_buffer.clear();

            regs.eax.set_zero();
            regs.edx.set_d(self.drive_spec.0 as u32);
            INT13.call(regs);

            if !Self::is_floppy(self.drive_spec) {
                regs.eax.set_d(0x4800);
                regs.edx.set_d(self.drive_spec.0 as u32);
                regs.set_vmds(packet_buffer.sel());
                regs.esi.set_zero();
                packet_buffer.as_slice()[0] = 0x1a;
                INT13.call(regs);
                if regs.eflags().is_nc() {
                    let p = packet_buffer.as_slice().as_ptr() as usize;

                    let flags = ((p + 2) as *const u16).read_volatile();
                    let c = ((p + 4) as *const u32).read_volatile();
                    let h = ((p + 8) as *const u32).read_volatile();
                    let s = ((p + 12) as *const u32).read_volatile();
                    let total_sectors = ((p + 16) as *const u64).read_volatile();
                    let block_size = ((p + 24) as *const u16).read_volatile();

                    self.geometry = if (flags & 0x0002) == 0 {
                        Geometry::EMPTY
                    } else {
                        Geometry {
                            c: (c as u16),
                            h: (h as u8),
                            s: s as u8,
                        }
                    };
                    self.media_info.block_count = if (flags & 0x0040) == 0 {
                        LBA(total_sectors)
                    } else {
                        LBA(0)
                    };
                    self.media_info.block_size = block_size as u32;
                    self.is_lba_supported = true;
                    return Ok(());
                }
            }
            self.is_lba_supported = false;

            regs.eax.set_d(0x0800);
            regs.edx.set_d(self.drive_spec.0 as u32);
            regs.set_vmes(Selector::NULL);
            regs.edi.set_zero();
            INT13.call(regs);
            if regs.eflags().is_c() {
                let error = Self::convert_status_code(regs.ah());
                self.geometry = Geometry::EMPTY;
                self.media_info.block_count = LBA(0);
                self.media_info.block_size = 0;
                return Err(error);
            }

            self.geometry = Geometry {
                c: (regs.ch() as u16 | ((regs.cl() as u16 & 0xc0) << 2)) + 1,
                h: regs.dh().saturating_add(1),
                s: regs.cl() & 0x3f,
            };
            self.media_info.block_count = LBA(self.geometry.total_sectors() as u64);
            self.media_info.block_size = 512;

            Ok(())
        }
    }

    fn convert_status_code(code: u8) -> BlockIoError {
        let code = Int13StatusCode::from_u8(code);
        match code {
            Int13StatusCode::InvalidParameter => BlockIoError::InvalidParameter,
            Int13StatusCode::AddressMarkNotFound => BlockIoError::DeviceError,
            Int13StatusCode::WriteProtected => BlockIoError::WriteProtected,
            Int13StatusCode::SectorNotFound => BlockIoError::DeviceError,
            Int13StatusCode::ResetFailed => BlockIoError::DeviceError,
            Int13StatusCode::DiskChanged => BlockIoError::MediaChanged,
            Int13StatusCode::DmaOverrun => BlockIoError::DeviceError,
            Int13StatusCode::DataBoundaryError => BlockIoError::DeviceError,
            Int13StatusCode::BadSectorDetected => BlockIoError::DeviceError,
            Int13StatusCode::BadTrackDetected => BlockIoError::DeviceError,
            Int13StatusCode::InvalidMedia => BlockIoError::DeviceError,
            Int13StatusCode::SeekFailed => BlockIoError::DeviceError,
            Int13StatusCode::TimedOut => BlockIoError::NoMedia,
            Int13StatusCode::DriveNotReady => BlockIoError::NoMedia,
            Int13StatusCode::Success | Int13StatusCode::UnknownError => BlockIoError::DeviceError,
        }
    }

    fn handle_error(&mut self, regs: &mut Vm86Context) -> Result<(), BlockIoError> {
        if regs.eflags().is_nc() {
            return Ok(());
        }
        let error = Self::convert_status_code(regs.ah());
        match error {
            BlockIoError::NoMedia => {
                self.geometry = Geometry::EMPTY;
                self.media_info.block_count = LBA(0);
                self.media_info.block_size = 0;
            }
            BlockIoError::MediaChanged => {
                self.media_info.media_id.succ();
                let _ = self.reset_media(regs);
            }
            _ => {}
        }
        Err(error)
    }

    /// Perform a read/write operation using INT13h.
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

            if self.is_lba_supported {
                let packet_buffer = disk_bios.packet_buffer.as_ref().unwrap();
                packet_buffer.clear();

                let lba_packet = packet_buffer.map::<LbaPacket>().unwrap();

                let mut left = bytes_left / block_size;
                while left > 0 {
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

                    lba_packet.init(io_buffer.as_far16(), 1, lba);
                    regs.eax.set_h(function.func_no() | 0x40);
                    regs.edx.set_d(self.drive_spec.0 as u32);
                    regs.set_vmds(packet_buffer.sel());
                    regs.esi.set_zero();
                    INT13.call(&mut regs);
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
                    left -= 1;
                }
                return Ok(());
            }

            while bytes_left > 0 {
                if lba >= self.media_info.block_count {
                    return Err(BlockIoError::InvalidParameter);
                }

                let chs = self
                    .geometry
                    .convert(lba)
                    .ok_or(BlockIoError::InvalidParameter)?;

                let transfer_secs = (bytes_left / block_size)
                    .min(self.geometry.max_sector() as usize - (chs.s as usize) + 1)
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

                let bios_geometry = BiosGeometryRegs::from_geometry(chs);
                regs.eax.set_hl(function.func_no(), transfer_secs);
                regs.ecx.set_d(bios_geometry.cx as u32);
                regs.edx.set_hl(bios_geometry.dh, self.drive_spec.0);
                regs.set_vmes(io_buffer.sel());
                regs.ebx.set_zero();
                INT13.call(&mut regs);
                self.handle_error(&mut regs)?;

                match function {
                    TransferFunction::Read(_) => {
                        (target_addr as *mut u8)
                            .copy_from_nonoverlapping(io_buffer.as_slice().as_ptr(), transfer_size);
                    }
                    TransferFunction::Write(_) => {
                        // No need to copy back for write operation
                    }
                }

                lba += transfer_secs as u64;
                target_addr += transfer_size;
                bytes_left -= transfer_size;
            }
        }

        Ok(())
    }
}

#[allow(dead_code)]
impl BlockDevice for Int13Device {
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
pub enum Int13StatusCode {
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
impl Int13StatusCode {
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

#[derive(Debug, Clone, Copy)]
pub struct BiosGeometryRegs {
    cx: u16,
    dh: u8,
}

impl BiosGeometryRegs {
    #[inline]
    pub fn from_geometry(geometry: Geometry) -> Self {
        let cx = ((geometry.c & 0x300) << 2) | (geometry.c & 0xff) | (geometry.s as u16 & 0x3f);
        let dh = geometry.h;
        Self { cx, dh }
    }
}

#[repr(C)]
pub struct LbaPacket {
    size: u8,
    reserved: u8,
    block_count: u16,
    buffer: Far16Ptr,
    start_lba: LBA,
}

impl LbaPacket {
    #[inline]
    pub fn init(&mut self, buffer: Far16Ptr, block_count: u16, start_lba: LBA) {
        self.size = core::mem::size_of::<Self>() as u8;
        self.reserved = 0;
        self.buffer = buffer;
        self.block_count = block_count;
        self.start_lba = start_lba;
    }
}

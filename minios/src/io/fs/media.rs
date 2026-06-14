//! Block Device definitions.

use core::ops::{Add, AddAssign};

pub trait BlockDevice {
    /// Resets the device
    fn reset(&mut self) -> Result<(), BlockIoError>;

    /// Reads blocks from the device.
    ///
    /// When error occurs, the buffer may be invalid.
    fn read(&mut self, lba: LBA, buffer: &mut [u8]) -> Result<(), BlockIoError>;

    /// Writes blocks to the device.
    ///
    /// When error occurs, the device may be in an inconsistent state.
    fn write(&mut self, lba: LBA, buffer: &[u8]) -> Result<(), BlockIoError>;

    /// Flushes any buffered data to the device
    fn flush(&mut self) -> Result<(), BlockIoError> {
        Ok(())
    }

    /// Returns information about the media
    fn media_info(&mut self) -> &MediaInfo;
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct LBA(pub u64);

impl Add<u64> for LBA {
    type Output = Self;

    #[inline]
    fn add(self, rhs: u64) -> Self::Output {
        LBA(self.0 + rhs)
    }
}

impl AddAssign<u64> for LBA {
    #[inline]
    fn add_assign(&mut self, rhs: u64) {
        self.0 += rhs;
    }
}

impl Add<LBA> for LBA {
    type Output = Self;

    #[inline]
    fn add(self, rhs: LBA) -> Self::Output {
        LBA(self.0 + rhs.0)
    }
}

impl AddAssign<LBA> for LBA {
    #[inline]
    fn add_assign(&mut self, rhs: LBA) {
        self.0 += rhs.0;
    }
}

/// CHRN geometry information.
///
/// Useful for floppy disk drives
#[derive(Clone, Copy)]
pub struct CHRN {
    pub n: u8,
    pub r: u8,
    pub h: u8,
    pub c: u8,
}

impl CHRN {
    pub const EMPTY: Self = Self {
        c: 0,
        h: 0,
        r: 0,
        n: 0,
    };

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.c == 0 && self.h == 0 && self.r == 0 && self.n == 0
    }

    #[inline]
    pub const fn new(c: u8, h: u8, r: u8, n: u8) -> Self {
        Self { c, h, r, n }
    }

    #[inline]
    pub const fn to_chs(&self) -> Geometry {
        Geometry {
            c: self.c as u16,
            h: self.h,
            s: self.r,
        }
    }

    #[inline]
    pub fn media_info_template(&self) -> MediaInfo {
        MediaInfo {
            media_id: MediaId::ZERO,
            flags: 0,
            block_size: 128 << self.n as u32,
            io_align: 1,
            block_count: LBA(((self.c as u32) * (self.h as u32) * (self.r as u32)) as u64),
        }
    }
}

impl core::fmt::Debug for CHRN {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CHRN({}:{}:{}:{})", self.c, self.h, self.r, self.n)
    }
}

/// CHS geometry information.
#[derive(Clone, Copy)]
pub struct Geometry {
    pub s: u8,
    pub h: u8,
    pub c: u16,
}

impl Geometry {
    /// An empty geometry with zero cylinders, heads, and sectors.
    pub const EMPTY: Self = Self { c: 0, h: 0, s: 0 };

    /// Returns `true` if the geometry is empty
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.c == 0 && self.h == 0 && self.s == 0
    }

    #[inline]
    pub const fn new(c: u16, h: u8, s: u8) -> Self {
        Self { c, h, s }
    }

    /// Returns max sector number (1-based)
    #[inline]
    pub const fn max_sector(&self) -> u8 {
        self.s
    }

    /// Returns heads per cylinder
    #[inline]
    pub const fn heads_per_cylinder(&self) -> u32 {
        self.h as u32
    }

    /// Returns max cylinder number (0-based)
    #[inline]
    pub const fn max_cylinder(&self) -> u16 {
        self.c
    }

    /// Calculate total sectors from CHS geometry.
    #[inline]
    pub const fn total_sectors(&self) -> u32 {
        (self.c as u32) * self.heads_per_cylinder() * (self.s as u32)
    }

    /// Convert LBA to CHS format.
    ///
    /// Returns `None` if LBA is out of range for the geometry.
    pub fn convert(&self, lba: LBA) -> Option<Geometry> {
        let total_sectors = self.total_sectors();
        if lba.0 >= (63 * 256 * 1024) || lba.0 >= total_sectors as u64 {
            return None;
        }
        let lba = lba.0 as u32;

        let s = ((lba % (self.s as u32)) + 1) as u8;
        let track = lba / (self.s as u32);
        let heads_per_cylinder = self.heads_per_cylinder();
        let h = (track % heads_per_cylinder) as u8;
        let c = (track / heads_per_cylinder) as u16;

        Some(Geometry { c, h, s })
    }
}

impl core::fmt::Debug for Geometry {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CHS({}:{}:{})", self.c, self.h, self.s)
    }
}

/// Media ID
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MediaId(pub u32);

impl MediaId {
    pub const ZERO: Self = Self(0);

    #[inline]
    pub fn succ(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockIoError {
    /// A generic error occurred
    DeviceError,
    /// The specified parameter is invalid
    InvalidParameter,
    /// The device is write-protected
    WriteProtected,
    /// No media is present
    NoMedia,
    /// The media has changed
    MediaChanged,
    /// The buffer size is not a multiple of the block size of the device
    BadBufferSize,
}

#[derive(Debug, Clone, Copy)]
pub struct MediaInfo {
    /// The media ID, which changes when the media is changed.
    pub media_id: MediaId,
    /// TBD
    pub flags: u32,
    /// The size of a block in bytes.
    pub block_size: u32,
    pub io_align: u32,
    /// The total number of blocks on the media.
    pub block_count: LBA,
}

impl MediaInfo {
    pub const EMPTY: Self = Self {
        media_id: MediaId(0),
        flags: 0,
        block_size: 0,
        io_align: 0,
        block_count: LBA(0),
    };
}

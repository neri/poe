//! Graphics related I/O

pub mod color;
pub mod display;

use crate::PhysicalAddress;

pub trait GraphicsOutput {
    fn modes(&self) -> &[ModeInfo];

    fn current_mode(&self) -> &CurrentMode;

    fn set_mode(&mut self, mode: ModeIndex) -> Result<(), ()>;

    fn detach(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModeIndex(pub usize);

#[derive(Debug)]
pub struct CurrentMode {
    pub current: ModeIndex,
    pub info: ModeInfo,
    pub fb: PhysicalAddress,
    pub fb_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeInfo {
    pub width: u16,
    pub height: u16,
    pub bytes_per_scanline: u16,
    pub pixel_format: PixelFormat,
}

impl CurrentMode {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            current: ModeIndex(0),
            info: ModeInfo {
                width: 0,
                height: 0,
                bytes_per_scanline: 0,
                pixel_format: PixelFormat::Indexed8,
            },
            fb: PhysicalAddress::new(0),
            fb_size: 0,
        }
    }
}

impl ModeInfo {
    #[inline]
    pub fn is_uefi_compatible(&self) -> bool {
        matches!(self.pixel_format, PixelFormat::BGRX8888) && (self.bytes_per_scanline & 3) == 0
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 8bit Indexed Color
    Indexed8 = 1,
    /// 32bit Color, ARGB in little endian.
    /// It is commonly used in UEFI GOP and VESA VBE.
    BGRX8888 = 2,
    /// 32bit Color, RGBA in big endian.
    /// It is commonly used in HTML canvas and general image processing.
    RGBX8888 = 3,
}

impl PixelFormat {
    #[inline]
    pub const fn is_indexed_color(&self) -> bool {
        match self {
            PixelFormat::Indexed8 => true,
            _ => false,
        }
    }

    #[inline]
    pub const fn bits_per_pixel(&self) -> usize {
        match self {
            PixelFormat::Indexed8 => 8,
            PixelFormat::BGRX8888 | PixelFormat::RGBX8888 => 32,
        }
    }

    #[inline]
    pub const fn bytes_per_pixel(&self) -> usize {
        (self.bits_per_pixel() + 7) / 8
    }
}

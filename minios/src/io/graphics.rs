//! Graphics related I/O

pub mod color;
pub mod display;
pub mod fbcon;

use crate::PhysicalAddress;

pub trait GraphicsOutputDevice {
    /// Returns the list of supported video modes.
    fn modes(&self) -> &[ModeInfo];

    /// Gets the current video mode information.
    fn current_mode(&self) -> &CurrentMode;

    /// Sets the video mode to the specified mode index.
    fn set_mode(&mut self, mode: ModeIndex) -> Result<(), ()>;

    /// Detaches the graphics output device.
    fn detach(&mut self);
}

/// Video mode index type.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModeIndex(pub usize);

/// Current video mode information.
#[derive(Debug)]
pub struct CurrentMode {
    /// Index of the current video mode.
    pub current: ModeIndex,
    /// Information about the current video mode.
    pub info: ModeInfo,
    /// Framebuffer physical address.
    pub fb: PhysicalAddress,
    /// Size of the framebuffer in bytes.
    pub fb_size: usize,
}

/// Video mode information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeInfo {
    /// Width of the display in pixels.
    pub width: u16,
    /// Height of the display in pixels.
    pub height: u16,
    /// Number of bytes per scanline.
    pub bytes_per_scanline: u16,
    /// Pixel format.
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
    /// Checks if the mode is compatible with UEFI GOP.
    #[inline]
    pub const fn is_uefi_compatible(&self) -> bool {
        matches!(self.pixel_format, PixelFormat::BGRX8888) && self.pixels_per_scanline().is_some()
    }

    /// Checks if the mode is compatible with HRB
    #[inline]
    pub const fn is_hrb_compatible(&self) -> bool {
        match self.pixels_per_scanline() {
            Some(stride) => self.width == stride,
            None => false,
        }
    }

    /// Returns the number of pixels per scanline, if applicable.
    pub const fn pixels_per_scanline(&self) -> Option<u16> {
        match self.pixel_format {
            PixelFormat::Indexed8 => Some(self.bytes_per_scanline),

            PixelFormat::BGRX8888 | PixelFormat::RGBX8888 => {
                if (self.bytes_per_scanline & 3) == 0 {
                    Some(self.bytes_per_scanline / 4)
                } else {
                    None
                }
            }

            #[allow(unreachable_patterns)]
            _ => None,
        }
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
    /// Returns whether the pixel format is indexed color.
    #[inline]
    pub const fn is_indexed_color(&self) -> bool {
        match self {
            PixelFormat::Indexed8 => true,
            _ => false,
        }
    }

    /// Returns bits per pixel for the pixel format.
    #[inline]
    pub const fn bits_per_pixel(&self) -> usize {
        match self {
            PixelFormat::Indexed8 => 8,
            PixelFormat::BGRX8888 | PixelFormat::RGBX8888 => 32,
        }
    }

    /// Returns typical bytes per pixel for the pixel format.
    #[inline]
    pub const fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelFormat::Indexed8 => 1,
            PixelFormat::BGRX8888 | PixelFormat::RGBX8888 => 4,
        }
    }
}

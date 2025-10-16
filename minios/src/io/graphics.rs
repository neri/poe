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

/// Standard 256 color palette (ARGB in little endian)
pub const COLOR_PALETTE: [u32; 256] = [
    0xFF212121, 0xFF0D47A1, 0xFF1B5E20, 0xFF006064, 0xFFB71C1C, 0xFF4A148C, 0xFF795548, 0xFFBDBDBD,
    0xFF616161, 0xFF2196F3, 0xFF4CAF50, 0xFF00BCD4, 0xFFF44336, 0xFF9C27B0, 0xFFFFEB3B, 0xFFFFFFFF,
    0xFF000000, 0xFF330000, 0xFF660000, 0xFF990000, 0xFFCC0000, 0xFFFF0000, 0xFF003300, 0xFF333300,
    0xFF663300, 0xFF993300, 0xFFCC3300, 0xFFFF3300, 0xFF006600, 0xFF336600, 0xFF666600, 0xFF996600,
    0xFFCC6600, 0xFFFF6600, 0xFF009900, 0xFF339900, 0xFF669900, 0xFF999900, 0xFFCC9900, 0xFFFF9900,
    0xFF00CC00, 0xFF33CC00, 0xFF66CC00, 0xFF99CC00, 0xFFCCCC00, 0xFFFFCC00, 0xFF00FF00, 0xFF33FF00,
    0xFF66FF00, 0xFF99FF00, 0xFFCCFF00, 0xFFFFFF00, 0xFF000033, 0xFF330033, 0xFF660033, 0xFF990033,
    0xFFCC0033, 0xFFFF0033, 0xFF003333, 0xFF333333, 0xFF663333, 0xFF993333, 0xFFCC3333, 0xFFFF3333,
    0xFF006633, 0xFF336633, 0xFF666633, 0xFF996633, 0xFFCC6633, 0xFFFF6633, 0xFF009933, 0xFF339933,
    0xFF669933, 0xFF999933, 0xFFCC9933, 0xFFFF9933, 0xFF00CC33, 0xFF33CC33, 0xFF66CC33, 0xFF99CC33,
    0xFFCCCC33, 0xFFFFCC33, 0xFF00FF33, 0xFF33FF33, 0xFF66FF33, 0xFF99FF33, 0xFFCCFF33, 0xFFFFFF33,
    0xFF000066, 0xFF330066, 0xFF660066, 0xFF990066, 0xFFCC0066, 0xFFFF0066, 0xFF003366, 0xFF333366,
    0xFF663366, 0xFF993366, 0xFFCC3366, 0xFFFF3366, 0xFF006666, 0xFF336666, 0xFF666666, 0xFF996666,
    0xFFCC6666, 0xFFFF6666, 0xFF009966, 0xFF339966, 0xFF669966, 0xFF999966, 0xFFCC9966, 0xFFFF9966,
    0xFF00CC66, 0xFF33CC66, 0xFF66CC66, 0xFF99CC66, 0xFFCCCC66, 0xFFFFCC66, 0xFF00FF66, 0xFF33FF66,
    0xFF66FF66, 0xFF99FF66, 0xFFCCFF66, 0xFFFFFF66, 0xFF000099, 0xFF330099, 0xFF660099, 0xFF990099,
    0xFFCC0099, 0xFFFF0099, 0xFF003399, 0xFF333399, 0xFF663399, 0xFF993399, 0xFFCC3399, 0xFFFF3399,
    0xFF006699, 0xFF336699, 0xFF666699, 0xFF996699, 0xFFCC6699, 0xFFFF6699, 0xFF009999, 0xFF339999,
    0xFF669999, 0xFF999999, 0xFFCC9999, 0xFFFF9999, 0xFF00CC99, 0xFF33CC99, 0xFF66CC99, 0xFF99CC99,
    0xFFCCCC99, 0xFFFFCC99, 0xFF00FF99, 0xFF33FF99, 0xFF66FF99, 0xFF99FF99, 0xFFCCFF99, 0xFFFFFF99,
    0xFF0000CC, 0xFF3300CC, 0xFF6600CC, 0xFF9900CC, 0xFFCC00CC, 0xFFFF00CC, 0xFF0033CC, 0xFF3333CC,
    0xFF6633CC, 0xFF9933CC, 0xFFCC33CC, 0xFFFF33CC, 0xFF0066CC, 0xFF3366CC, 0xFF6666CC, 0xFF9966CC,
    0xFFCC66CC, 0xFFFF66CC, 0xFF0099CC, 0xFF3399CC, 0xFF6699CC, 0xFF9999CC, 0xFFCC99CC, 0xFFFF99CC,
    0xFF00CCCC, 0xFF33CCCC, 0xFF66CCCC, 0xFF99CCCC, 0xFFCCCCCC, 0xFFFFCCCC, 0xFF00FFCC, 0xFF33FFCC,
    0xFF66FFCC, 0xFF99FFCC, 0xFFCCFFCC, 0xFFFFFFCC, 0xFF0000FF, 0xFF3300FF, 0xFF6600FF, 0xFF9900FF,
    0xFFCC00FF, 0xFFFF00FF, 0xFF0033FF, 0xFF3333FF, 0xFF6633FF, 0xFF9933FF, 0xFFCC33FF, 0xFFFF33FF,
    0xFF0066FF, 0xFF3366FF, 0xFF6666FF, 0xFF9966FF, 0xFFCC66FF, 0xFFFF66FF, 0xFF0099FF, 0xFF3399FF,
    0xFF6699FF, 0xFF9999FF, 0xFFCC99FF, 0xFFFF99FF, 0xFF00CCFF, 0xFF33CCFF, 0xFF66CCFF, 0xFF99CCFF,
    0xFFCCCCFF, 0xFFFFCCFF, 0xFF00FFFF, 0xFF33FFFF, 0xFF66FFFF, 0xFF99FFFF, 0xFFCCFFFF, 0xFFFFFFFF,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

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

//! Color types and palette

use embedded_graphics::pixelcolor::raw::RawU8;
use embedded_graphics::prelude::*;

pub trait PrimaryColor: PixelColor {
    /// RGB (0, 0, 0)
    const PRIMARY_BLACK: Self;
    /// RGB (0, 0, 1)
    const PRIMARY_BLUE: Self;
    /// RGB (0, 1, 0)
    const PRIMARY_GREEN: Self;
    /// RGB (0, 1, 1)
    const PRIMARY_CYAN: Self;
    /// RGB (1, 0, 0)
    const PRIMARY_RED: Self;
    /// RGB (1, 0, 1)
    const PRIMARY_MAGENTA: Self;
    /// RGB (1, 1, 0)
    const PRIMARY_YELLOW: Self;
    /// RGB (1, 1, 1)
    const PRIMARY_WHITE: Self;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IndexedColor(pub u8);

impl IndexedColor {
    pub const MIN: Self = Self(u8::MIN);
    pub const MAX: Self = Self(u8::MAX);

    pub const BLACK: Self = Self(0);
    pub const BLUE: Self = Self(1);
    pub const GREEN: Self = Self(2);
    pub const CYAN: Self = Self(3);
    pub const RED: Self = Self(4);
    pub const MAGENTA: Self = Self(5);
    pub const BROWN: Self = Self(6);
    pub const LIGHT_GRAY: Self = Self(7);
    pub const DARK_GRAY: Self = Self(8);
    pub const LIGHT_BLUE: Self = Self(9);
    pub const LIGHT_GREEN: Self = Self(10);
    pub const LIGHT_CYAN: Self = Self(11);
    pub const LIGHT_RED: Self = Self(12);
    pub const LIGHT_MAGENTA: Self = Self(13);
    pub const YELLOW: Self = Self(14);
    pub const WHITE: Self = Self(15);

    /// Standard 256 color palette (RGB in little endian)
    pub const COLOR_PALETTE: [u32; 256] = [
        0x000000, 0x000099, 0x009900, 0x009999, 0x990000, 0x990099, 0x999900, 0x999999, 0x666666,
        0x0000ff, 0x00ff00, 0x00ffff, 0xff0000, 0xff00ff, 0xffff00, 0xffffff, 0x000000, 0x000033,
        0x000066, 0x000099, 0x0000cc, 0x0000ff, 0x003300, 0x003333, 0x003366, 0x003399, 0x0033cc,
        0x0033ff, 0x006600, 0x006633, 0x006666, 0x006699, 0x0066cc, 0x0066ff, 0x009900, 0x009933,
        0x009966, 0x009999, 0x0099cc, 0x0099ff, 0x00cc00, 0x00cc33, 0x00cc66, 0x00cc99, 0x00cccc,
        0x00ccff, 0x00ff00, 0x00ff33, 0x00ff66, 0x00ff99, 0x00ffcc, 0x00ffff, 0x330000, 0x330033,
        0x330066, 0x330099, 0x3300cc, 0x3300ff, 0x333300, 0x333333, 0x333366, 0x333399, 0x3333cc,
        0x3333ff, 0x336600, 0x336633, 0x336666, 0x336699, 0x3366cc, 0x3366ff, 0x339900, 0x339933,
        0x339966, 0x339999, 0x3399cc, 0x3399ff, 0x33cc00, 0x33cc33, 0x33cc66, 0x33cc99, 0x33cccc,
        0x33ccff, 0x33ff00, 0x33ff33, 0x33ff66, 0x33ff99, 0x33ffcc, 0x33ffff, 0x660000, 0x660033,
        0x660066, 0x660099, 0x6600cc, 0x6600ff, 0x663300, 0x663333, 0x663366, 0x663399, 0x6633cc,
        0x6633ff, 0x666600, 0x666633, 0x666666, 0x666699, 0x6666cc, 0x6666ff, 0x669900, 0x669933,
        0x669966, 0x669999, 0x6699cc, 0x6699ff, 0x66cc00, 0x66cc33, 0x66cc66, 0x66cc99, 0x66cccc,
        0x66ccff, 0x66ff00, 0x66ff33, 0x66ff66, 0x66ff99, 0x66ffcc, 0x66ffff, 0x990000, 0x990033,
        0x990066, 0x990099, 0x9900cc, 0x9900ff, 0x993300, 0x993333, 0x993366, 0x993399, 0x9933cc,
        0x9933ff, 0x996600, 0x996633, 0x996666, 0x996699, 0x9966cc, 0x9966ff, 0x999900, 0x999933,
        0x999966, 0x999999, 0x9999cc, 0x9999ff, 0x99cc00, 0x99cc33, 0x99cc66, 0x99cc99, 0x99cccc,
        0x99ccff, 0x99ff00, 0x99ff33, 0x99ff66, 0x99ff99, 0x99ffcc, 0x99ffff, 0xcc0000, 0xcc0033,
        0xcc0066, 0xcc0099, 0xcc00cc, 0xcc00ff, 0xcc3300, 0xcc3333, 0xcc3366, 0xcc3399, 0xcc33cc,
        0xcc33ff, 0xcc6600, 0xcc6633, 0xcc6666, 0xcc6699, 0xcc66cc, 0xcc66ff, 0xcc9900, 0xcc9933,
        0xcc9966, 0xcc9999, 0xcc99cc, 0xcc99ff, 0xcccc00, 0xcccc33, 0xcccc66, 0xcccc99, 0xcccccc,
        0xccccff, 0xccff00, 0xccff33, 0xccff66, 0xccff99, 0xccffcc, 0xccffff, 0xff0000, 0xff0033,
        0xff0066, 0xff0099, 0xff00cc, 0xff00ff, 0xff3300, 0xff3333, 0xff3366, 0xff3399, 0xff33cc,
        0xff33ff, 0xff6600, 0xff6633, 0xff6666, 0xff6699, 0xff66cc, 0xff66ff, 0xff9900, 0xff9933,
        0xff9966, 0xff9999, 0xff99cc, 0xff99ff, 0xffcc00, 0xffcc33, 0xffcc66, 0xffcc99, 0xffcccc,
        0xffccff, 0xffff00, 0xffff33, 0xffff66, 0xffff99, 0xffffcc, 0xffffff, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];

    #[inline]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    #[inline]
    pub const fn into_raw(self) -> u8 {
        self.0
    }

    #[inline]
    pub const fn from_rgb(rgb: u32) -> Self {
        let b = (((rgb & 0xff) + 25) / 51) as u8;
        let g = ((((rgb >> 8) & 0xff) + 25) / 51) as u8;
        let r = ((((rgb >> 16) & 0xff) + 25) / 51) as u8;
        Self(16 + r * 36 + g * 6 + b)
    }
}

impl PixelColor for IndexedColor {
    type Raw = RawU8;
}

impl PrimaryColor for IndexedColor {
    const PRIMARY_BLACK: Self = Self::from_rgb(0x00_00_00);
    const PRIMARY_BLUE: Self = Self::from_rgb(0x00_00_FF);
    const PRIMARY_GREEN: Self = Self::from_rgb(0x00_FF_00);
    const PRIMARY_CYAN: Self = Self::from_rgb(0x00_FF_FF);
    const PRIMARY_RED: Self = Self::from_rgb(0xFF_00_00);
    const PRIMARY_MAGENTA: Self = Self::from_rgb(0xFF_00_FF);
    const PRIMARY_YELLOW: Self = Self::from_rgb(0xFF_FF_00);
    const PRIMARY_WHITE: Self = Self::from_rgb(0xFF_FF_FF);
}

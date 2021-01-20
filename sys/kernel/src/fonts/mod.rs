// Fonts

use super::graphics::bitmap::*;
use crate::graphics::coords::*;

// include!("megbtan.rs");
include!("megh0816.rs");
#[allow(dead_code)]
const SYSTEM_FONT: FixedFontDriver = FixedFontDriver::new(8, 16, &FONT_MEGH0816_DATA);

include!("megh0608.rs");
#[allow(dead_code)]
const SMALL_FONT: FixedFontDriver = FixedFontDriver::new(6, 8, &FONT_MEGH0608_DATA);

pub struct FontManager {}

impl FontManager {
    #[inline]
    pub const fn fixed_system_font() -> &'static FixedFontDriver<'static> {
        &SYSTEM_FONT
    }

    #[inline]
    pub const fn fixed_small_font() -> &'static FixedFontDriver<'static> {
        &SMALL_FONT
    }
}

pub struct FixedFontDriver<'a> {
    size: Size,
    data: &'a [u8],
    leading: isize,
    line_height: isize,
    stride: usize,
}

impl FixedFontDriver<'_> {
    pub const fn new(width: usize, height: usize, data: &'static [u8]) -> FixedFontDriver<'static> {
        let width = width as isize;
        let height = height as isize;
        let line_height = height * 5 / 4;
        let leading = (line_height - height) / 2;
        let stride = ((width as usize + 7) >> 3) * height as usize;
        FixedFontDriver {
            size: Size::new(width, height),
            line_height,
            leading,
            stride,
            data,
        }
    }

    #[inline]
    pub const fn width(&self) -> isize {
        self.size.width
    }

    #[inline]
    pub const fn line_height(&self) -> isize {
        self.line_height
    }

    /// Glyph Data for Rasterized Font
    pub fn glyph_for(&self, character: char) -> Option<&[u8]> {
        let c = character as usize;
        if c > 0x20 && c < 0x80 {
            let base = self.stride * (c - 0x20);
            Some(&self.data[base..base + self.stride])
        } else {
            None
        }
    }

    /// Write character to bitmap
    pub fn write_char<T>(&self, character: char, to: &mut T, origin: Point, color: T::PixelType)
    where
        T: RasterFontWriter,
    {
        if let Some(font) = self.glyph_for(character) {
            let origin = Point::new(origin.x, origin.y + self.leading);
            to.draw_font(font, self.size, origin, color);
        }
    }

    /// Write string to bitmap
    pub fn write_str<T>(&self, s: &str, to: &mut T, origin: Point, color: T::PixelType)
    where
        T: RasterFontWriter,
    {
        let mut origin = origin;
        for c in s.chars() {
            self.write_char(c, to, origin, color);
            origin.x += self.size.width();
        }
    }
}

// Fonts

use super::graphics::bitmap::*;
use crate::graphics::coords::*;

include!("megh0816.rs");
const SYSTEM_FONT: FixedFontDriver = FixedFontDriver::new(8, 16, &FONT_MEGH0816_DATA, None);

include!("megh0608.rs");
const SMALL_FONT: FixedFontDriver = FixedFontDriver::new(6, 8, &FONT_MEGH0608_DATA, None);

include!("megmsgr2.rs");
const SYSTEM_UI_FONT: FixedFontDriver =
    FixedFontDriver::new(8, 12, &FONT_MEGMSGR2_DATA, Some(SYSTEM_UI_WIDTH_TABLE));

const SYSTEM_UI_WIDTH_TABLE: [u8; 96] = [
    6, 4, 6, 7, 6, 7, 7, 3, 4, 4, 6, 6, 3, 6, 3, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 5, 5, 5, 6, 5, 6,
    7, 7, 7, 7, 7, 6, 6, 7, 7, 4, 6, 7, 6, 8, 7, 7, 7, 7, 7, 7, 6, 7, 6, 8, 7, 6, 7, 4, 5, 4, 7, 6,
    3, 6, 6, 6, 6, 6, 5, 6, 6, 5, 5, 6, 5, 8, 6, 6, 6, 6, 5, 6, 5, 6, 6, 8, 6, 6, 6, 4, 2, 4, 6, 6,
];

pub struct FontManager {}

impl FontManager {
    #[inline]
    pub const fn fixed_system_font() -> &'static FixedFontDriver<'static> {
        &SYSTEM_FONT
    }

    #[inline]
    pub const fn fixed_ui_font() -> &'static FixedFontDriver<'static> {
        &SYSTEM_UI_FONT
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
    width_table: Option<[u8; 96]>,
}

impl FixedFontDriver<'_> {
    pub const fn new(
        width: usize,
        height: usize,
        data: &'static [u8],
        width_table: Option<[u8; 96]>,
    ) -> FixedFontDriver<'static> {
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
            width_table,
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

    #[inline]
    pub fn width_for(&self, character: char) -> isize {
        if let Some(width_table) = self.width_table {
            let c = character as usize;
            if c >= 0x20 && c < 0x80 {
                width_table[c - 0x20] as isize
            } else {
                self.width()
            }
        } else {
            self.width()
        }
    }

    #[inline]
    pub fn height_for(&self, character: char) -> isize {
        let _ = character;
        self.size.height
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
            let size = Size::new(self.width_for(character), self.size.height());
            to.draw_font(font, size, origin, color);
        }
    }
}

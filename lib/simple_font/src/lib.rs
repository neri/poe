//! Simple font and glyph
//!
#![cfg_attr(not(test), no_std)]

pub use font_macro::*;
pub mod mapping;
use crate::mapping::GlyphMapping;

pub struct SimpleFont<'a> {
    data: &'a [u8],
    dims: (u32, u32),
    char_stride: usize,
    glyph_mapping: &'a dyn GlyphMapping,
}

impl<'a> SimpleFont<'a> {
    #[inline]
    pub const fn ascii(data: &'a [u8], dims: (u32, u32)) -> Self {
        Self::new(data, dims, &mapping::ASCII)
    }

    #[inline]
    pub const fn new(
        data: &'a [u8],
        dims: (u32, u32),
        glyph_mapping: &'a dyn GlyphMapping,
    ) -> Self {
        let char_stride = ((dims.0 as usize + 7) / 8) * dims.1 as usize;
        Self {
            data,
            dims,
            char_stride,
            glyph_mapping,
        }
    }

    pub fn glyph_for_char(&self, ch: char) -> Option<SimpleGlyph<'a>> {
        let glyph_index = self.glyph_mapping.map_char(ch)?;
        let base = glyph_index * self.char_stride;
        let data = self.data.get(base..base + self.char_stride)?;
        Some(SimpleGlyph {
            data,
            dims: self.dims,
        })
    }

    #[inline]
    pub const fn font_width(&self) -> u32 {
        self.dims.0
    }

    #[inline]
    pub const fn font_height(&self) -> u32 {
        self.dims.1
    }
}

pub struct SimpleGlyph<'a> {
    pub data: &'a [u8],
    pub dims: (u32, u32),
}

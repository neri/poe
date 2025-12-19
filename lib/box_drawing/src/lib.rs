//! Box Drawing Characters Library

#![cfg_attr(not(test), no_std)]

#[path = "_generated/box_drawing.rs"]
mod box_drawing;
pub use box_drawing::*;

pub mod dir;

#[cfg(test)]
mod tests;

impl AsciiExt {
    /// The replacement character for unsupported characters.
    pub const REPLACEMENT_CHARACTER: Self = Self(b'?');

    /// Convert to shift outed state and ASCII byte.
    ///
    /// # Returns
    ///
    /// A tuple of (is_shift_outed, ascii_byte).
    ///
    /// This character must be displayed on the terminal by properly executing the SI/SO sequence.
    #[inline]
    pub const fn to_shift_out_and_ascii(&self) -> (bool, u8) {
        (self.0 >= 0x80, self.0 & 0x7f)
    }
}

impl BoxDrawingChar {
    /// Combine two `BoxDrawingChar` by bitwise OR
    #[inline]
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        BoxDrawingChar::from_udlr(self.udlr() | rhs.udlr())
    }

    /// Get the directions of the lines connected to this character
    #[inline]
    pub const fn line_dirs(&self) -> dir::LineDirections {
        dir::LineDirections(self.udlr())
    }

    /// Create a `BoxDrawingChar` from line directions
    #[inline]
    pub const fn from_line_dirs(dirs: dir::LineDirections) -> Option<Self> {
        BoxDrawingChar::from_udlr(dirs.0)
    }

    /// Get the mirrored character
    #[inline]
    pub const fn mirrored(&self) -> Option<Self> {
        BoxDrawingChar::from_udlr(self.line_dirs().rev().0)
    }
}

//! Text User Interface Library

#![cfg_attr(not(test), no_std)]

pub mod buffer;
pub mod color;
pub mod coord;
pub mod fixed_str;

#[allow(unused)]
pub mod prelude {
    pub use crate::TChar;
    pub use crate::TuiDrawTarget;
    pub use crate::buffer::*;
    pub use crate::color::*;
    pub use crate::coord::*;
    pub use box_drawing;
}

extern crate alloc;

use box_drawing::BoxAscii;

pub trait TuiDrawTarget {
    fn draw(&mut self, origin: coord::Point, text: &str, attr: color::TuiAttribute);
}

pub trait TChar: Sized + Clone + Copy + PartialEq + Eq {
    fn from_char(c: char) -> Self;

    fn into_char(self) -> char;
}

impl TChar for BoxAscii {
    #[inline]
    fn from_char(c: char) -> Self {
        Self::from_char(c).unwrap_or(Self(b'?'))
    }

    #[inline]
    fn into_char(self) -> char {
        self.to_char().unwrap_or('?')
    }
}

impl TChar for u16 {
    #[inline]
    fn from_char(c: char) -> Self {
        match c as u32 {
            0..=0xd7ff | 0xe000..=0xffff => c as u16,
            // 0xd800..=0xdfff => /* surrogate halves */
            _ => 0xfffd,
        }
    }

    #[inline]
    fn into_char(self) -> char {
        char::from_u32(self as u32).unwrap_or('?')
    }
}

impl TChar for u32 {
    #[inline]
    fn from_char(c: char) -> Self {
        c as u32
    }

    #[inline]
    fn into_char(self) -> char {
        char::from_u32(self as u32).unwrap_or('?')
    }
}

impl TChar for char {
    #[inline]
    fn from_char(c: char) -> Self {
        c
    }

    #[inline]
    fn into_char(self) -> char {
        self
    }
}

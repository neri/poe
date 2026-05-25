//! Text User Interface Library

#![cfg_attr(not(test), no_std)]

pub mod buffer;
pub mod color;
pub mod coord;
pub mod fixed_str;
// pub mod region;

#[allow(unused)]
pub mod prelude {
    pub use box_drawing;

    pub use crate::buffer::*;
    pub use crate::color::*;
    pub use crate::coord::*;
    pub use crate::{TChar, TuiDrawTarget};
}

extern crate alloc;

use box_drawing::AsciiExt;

pub trait TuiDrawTarget {
    fn draw(&mut self, origin: coord::Point, text: &str, attr: color::TuiAttribute);
}

pub trait TChar: Sized + Clone + Copy + PartialEq + Eq {
    fn from_char(c: char) -> Self;

    fn into_char(self) -> char;
}

impl TChar for AsciiExt {
    #[inline]
    fn from_char(c: char) -> Self {
        Self::from_char(c).unwrap_or(Self::REPLACEMENT_CHARACTER)
    }

    #[inline]
    fn into_char(self) -> char {
        self.to_char()
    }
}

impl TChar for u16 {
    #[inline]
    fn from_char(c: char) -> Self {
        match c {
            '\0'..='\u{d7ff}' | '\u{e000}'..='\u{ffff}' => c as u16,
            _ => char::REPLACEMENT_CHARACTER as u16,
        }
    }

    #[inline]
    fn into_char(self) -> char {
        char::from_u32(self as u32).unwrap_or(char::REPLACEMENT_CHARACTER)
    }
}

impl TChar for u32 {
    #[inline]
    fn from_char(c: char) -> Self {
        c as u32
    }

    #[inline]
    fn into_char(self) -> char {
        char::from_u32(self as u32).unwrap_or(char::REPLACEMENT_CHARACTER)
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

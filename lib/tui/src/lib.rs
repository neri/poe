//! Text User Interface Library

#![cfg_attr(not(test), no_std)]

pub mod buffer;
pub mod coord;
pub mod fixed_str;

extern crate alloc;
use alloc::vec::Vec;

pub trait DrawTarget {
    fn draw(&mut self, origin: coord::Point, text: &str, attr: TuiAttribute);
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct TuiAttribute(pub u8);

pub trait TChar: Sized + Clone + Copy + PartialEq + Eq {
    fn from_char(c: char) -> Self;

    fn into_char(self) -> char;
}

impl TChar for u8 {
    #[inline]
    fn from_char(c: char) -> Self {
        if c.is_ascii() { c as u8 } else { b'?' }
    }

    #[inline]
    fn into_char(self) -> char {
        self as char
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

//! VT100 Terminal Driver

use super::*;
use crate::System;
use core::fmt::Write;

const COLOR_TABLE: [u8; 8] = [0, 4, 2, 6, 1, 3, 5, 7];

pub struct VT100<'a> {
    inner: &'a mut dyn SimpleTextOutput,
}

impl<'a> VT100<'a> {
    #[inline]
    pub const fn new(inner: &'a mut dyn SimpleTextOutput) -> Self {
        Self { inner }
    }
}

impl Write for VT100<'_> {
    #[inline]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.inner.write_str(s)
    }
}

impl SimpleTextOutput for VT100<'_> {
    fn reset(&mut self) {
        let _ = self.inner.write_str("\x1b[2J\x1b[H");
        self.inner.reset();
    }

    fn set_attribute(&mut self, attribute: u8) {
        let attribute = if attribute == 0 {
            System::DEFAULT_STDOUT_ATTRIBUTE
        } else {
            attribute
        };

        let fg = COLOR_TABLE[attribute as usize & 0x07];
        let bg = COLOR_TABLE[(attribute >> 4) as usize & 0x07];
        let bright = (attribute >> 7) & 0x01;
        let _ = if bright != 0 {
            self.inner
                .write_fmt(format_args!("\x1b[1;{};{}m", 30 + fg, 40 + bg))
        } else {
            self.inner
                .write_fmt(format_args!("\x1b[0;{};{}m", 30 + fg, 40 + bg))
        };
        self.inner.set_attribute(attribute);
    }

    fn clear_screen(&mut self) {
        let _ = self.inner.write_str("\x1b[2J\x1b[H");
        self.inner.clear_screen();
    }

    fn set_cursor_position(&mut self, col: u32, row: u32) {
        let _ = self
            .inner
            .write_fmt(format_args!("\x1b[{};{}H", row + 1, col + 1));
        self.inner.set_cursor_position(col, row);
    }

    fn enable_cursor(&mut self, visible: bool) -> bool {
        let _ = if visible {
            self.inner.write_str("\x1b[?25h")
        } else {
            self.inner.write_str("\x1b[?25l")
        };
        self.inner.enable_cursor(visible)
    }

    fn current_mode(&self) -> SimpleTextOutputMode {
        self.inner.current_mode()
    }
}

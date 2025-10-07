//! VT100 Terminal Driver

use super::*;
use crate::System;
use core::fmt::Write;

const COLOR_TABLE: [u8; 8] = [0, 4, 2, 6, 1, 5, 3, 7];

pub struct VT100<'a> {
    inner: VT100Inner<'a>,
    mode: SimpleTextOutputMode,
}

struct VT100Inner<'a>(&'a mut dyn SerialIo);

impl<'a> VT100<'a> {
    #[inline]
    pub const fn new(inner: &'a mut dyn SerialIo) -> Self {
        Self {
            inner: VT100Inner(inner),
            mode: SimpleTextOutputMode::default(),
        }
    }

    #[inline]
    pub fn wait_response(&mut self, expected: &[u8]) -> Option<u8> {
        while let Some(ch) = self.inner.0.read_byte() {
            if expected.contains(&ch) {
                return Some(ch);
            }
        }
        None
    }

    pub fn wait_byte(&mut self) -> u8 {
        loop {
            if let Some(ch) = self.inner.0.read_byte() {
                return ch;
            }
        }
    }

    pub fn get_cursor_position(&mut self) -> Option<(u8, u8)> {
        // TODO: timeout
        self.inner.0.flush_input();
        let _ = self.inner.write_str("\x1b[6n");
        let mut buf = [0u8; 16];
        let mut i = 0;
        while i < buf.len() {
            let b = self.wait_byte();
            buf[i] = b;
            i += 1;
            if b == b'R' {
                break;
            }
        }
        if i < 6 || buf[0] != 0x1b || buf[1] != b'[' || buf[i - 1] != b'R' {
            return None;
        }
        let mut semicolon_index = None;
        for j in 2..i - 1 {
            if buf[j] == b';' {
                semicolon_index = Some(j);
                break;
            }
        }
        let semicolon_index = semicolon_index?;
        let row = core::str::from_utf8(&buf[2..semicolon_index]).ok()?;
        let col = core::str::from_utf8(&buf[semicolon_index + 1..i - 1]).ok()?;
        let row: u8 = row.parse().ok()?;
        let col: u8 = col.parse().ok()?;
        Some((col - 1, row - 1))
    }

    #[inline]
    pub fn update_cursor_position(&mut self) {
        if let Some((col, row)) = self.get_cursor_position() {
            self.mode.cursor_column = col;
            self.mode.cursor_row = row;
        }
    }
}

impl Write for VT100<'_> {
    #[inline]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.inner.write_str(s)
    }
}

impl Write for VT100Inner<'_> {
    #[inline]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.0.write_bytes(s.as_bytes());
        Ok(())
    }
}

impl SimpleTextOutput for VT100<'_> {
    fn reset(&mut self) {
        let _ = self.inner.write_str("\x1b[255;255H");
        if let Some((col, row)) = self.get_cursor_position() {
            self.mode.columns = col.saturating_add(1);
            self.mode.rows = row.saturating_add(1);
        }
        // let _ = self.inner.write_str("\x1bc");
        self.set_attribute(0);
        let _ = self.inner.write_str("\x1b[2J\x1b[H");
        self.mode.cursor_column = 0;
        self.mode.cursor_row = 0;
        self.inner.0.reset();
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
        self.mode.attribute = attribute;
    }

    fn clear_screen(&mut self) {
        let _ = self.inner.write_str("\x1b[2J\x1b[H");
        self.mode.cursor_column = 0;
        self.mode.cursor_row = 0;
    }

    fn set_cursor_position(&mut self, col: u32, row: u32) {
        let _ = self
            .inner
            .write_fmt(format_args!("\x1b[{};{}H", row + 1, col + 1));
        self.mode.cursor_column = col as u8;
        self.mode.cursor_row = row as u8;
    }

    fn enable_cursor(&mut self, visible: bool) -> bool {
        let old_cursor_visible = self.mode.is_cursor_visible();
        let _ = if visible {
            self.inner.write_str("\x1b[?25h")
        } else {
            self.inner.write_str("\x1b[?25l")
        };
        self.mode.set_cursor_visible(visible);
        old_cursor_visible
    }

    fn current_mode(&mut self) -> SimpleTextOutputMode {
        self.update_cursor_position();
        self.mode.clone()
    }
}

impl SimpleTextInput for VT100<'_> {
    fn reset(&mut self) {
        self.inner.0.reset();
    }

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        match self.inner.0.read_byte() {
            Some(ch) => NonZeroInputKey::new(0xffff, ch as u16),
            None => None,
        }
    }
}

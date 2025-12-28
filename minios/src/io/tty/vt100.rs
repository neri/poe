//! VT100 Terminal Driver

use super::*;
use crate::System;
use core::fmt::Write;
use tui::prelude::box_drawing::AsciiExt;

const COLOR_TABLE: [u8; 8] = [0, 4, 2, 6, 1, 5, 3, 7];

pub struct VT100Err<'a> {
    inner: VT100Inner<'a>,
    mode: SimpleTextOutputMode,
    is_shifted_out: bool,
    charset: CharsetMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum CharsetMode {
    /// Terminal supports box drawing characters via VT100 charset switching
    AsciiBoxChar,
    /// Terminal supports pure ASCII characters only
    AsciiFallback,
    /// Terminal supports UTF-8
    UTF8,
}

struct VT100Inner<'a>(&'a mut dyn SerialIo);

impl<'a> VT100Err<'a> {
    #[inline]
    pub const fn new(inner: &'a mut dyn SerialIo) -> Self {
        Self {
            inner: VT100Inner(inner),
            mode: SimpleTextOutputMode::default(),
            is_shifted_out: false,
            charset: CharsetMode::AsciiBoxChar,
        }
    }

    #[inline]
    pub const fn with_charset(inner: &'a mut dyn SerialIo, charset: CharsetMode) -> Self {
        Self {
            inner: VT100Inner(inner),
            mode: SimpleTextOutputMode::default(),
            is_shifted_out: false,
            charset,
        }
    }

    #[inline]
    pub fn reset_with_charset(&mut self, charset: CharsetMode) {
        self.charset = charset;
        (self as &mut dyn SimpleTextOutput).reset();
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

impl Write for VT100Err<'_> {
    #[inline]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        match self.charset {
            CharsetMode::AsciiBoxChar => {
                for ch in s.chars() {
                    if let Some(ch) = AsciiExt::from_char(ch) {
                        let (is_so, ch) = ch.to_shift_out_and_ascii();
                        if is_so != self.is_shifted_out {
                            if is_so {
                                self.inner.0.write_byte(b'\x0e');
                            } else {
                                self.inner.0.write_byte(b'\x0f');
                            }
                            self.is_shifted_out = is_so;
                        }
                        self.inner.0.write_byte(ch);
                    } else {
                        self.inner.0.write_byte(b'?');
                    }
                }
                Ok(())
            }
            CharsetMode::AsciiFallback => {
                for ch in s.chars() {
                    if let Some(ch) = AsciiExt::from_char(ch) {
                        let ch = ch.to_ascii_fallback();
                        self.inner.0.write_byte(ch);
                    } else {
                        self.inner.0.write_byte(b'?');
                    }
                }
                Ok(())
            }
            CharsetMode::UTF8 => self.inner.write_str(s),
        }
    }
}

impl Write for VT100Inner<'_> {
    #[inline]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.0.write_bytes(s.as_bytes());
        Ok(())
    }
}

impl SimpleTextOutput for VT100Err<'_> {
    fn reset(&mut self) {
        // reset and get terminal size
        let _ = self.inner.write_str("\x1bc\x1b[255;255H");
        if let Some((col, row)) = self.get_cursor_position() {
            self.mode.columns = col.saturating_add(1);
            self.mode.rows = row.saturating_add(1);
        }

        // clear screen
        self.set_attribute(0);
        let _ = self.inner.write_str("\x1b[H");
        for row in 0..self.mode.rows {
            if row > 0 {
                let _ = self.inner.write_char('\n');
            }
            for _ in 0..self.mode.columns {
                let _ = self.inner.write_char(' ');
            }
        }

        // enable box drawing charset
        // if matches!(self.charset, CharsetMode::AsciiBoxChar) {
        let _ = self.inner.write_str("\x1b(B\x1b)0\x0f");
        self.is_shifted_out = false;
        // }

        // set cursor to home
        self.set_cursor_position(0, 0);

        // reset inner tty
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
        let fg = if (attribute & 0x08) != 0 {
            90 + fg
        } else {
            30 + fg
        };
        let bg = if (attribute & 0x80) != 0 {
            100 + bg
        } else {
            40 + bg
        };
        let _ = self.inner.write_fmt(format_args!("\x1b[{};{}m", fg, bg));
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

pub struct VT100<'a> {
    inner: VT100Err<'a>,
}

impl<'a> VT100<'a> {
    #[inline]
    pub const fn new(inner: &'a mut dyn SerialIo) -> Self {
        Self {
            inner: VT100Err::new(inner),
        }
    }
}

impl Write for VT100<'_> {
    #[inline]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.inner.write_str(s)
    }
}

impl SimpleTextOutput for VT100<'_> {
    #[inline]
    fn reset(&mut self) {
        self.inner.reset();
    }

    #[inline]
    fn set_attribute(&mut self, attribute: u8) {
        self.inner.set_attribute(attribute);
    }

    #[inline]
    fn clear_screen(&mut self) {
        self.inner.clear_screen();
    }

    #[inline]
    fn set_cursor_position(&mut self, col: u32, row: u32) {
        self.inner.set_cursor_position(col, row);
    }

    #[inline]
    fn enable_cursor(&mut self, visible: bool) -> bool {
        self.inner.enable_cursor(visible)
    }

    #[inline]
    fn current_mode(&mut self) -> SimpleTextOutputMode {
        self.inner.current_mode()
    }
}

impl SimpleTextInput for VT100<'_> {
    fn reset(&mut self) {
        self.inner.inner.0.reset();
    }

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        match self.inner.inner.0.read_byte() {
            Some(ch) => NonZeroInputKey::new(0xffff, ch as u16),
            None => None,
        }
    }
}

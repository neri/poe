//! VT100 Serial Terminal Driver

use super::*;
use crate::*;
use core::time::Duration;
use tui::prelude::box_drawing::AsciiExt;

/// Color mapping from 3-bit color attributes to VT100 color codes.
const COLOR_TABLE: [u8; 8] = [0, 4, 2, 6, 1, 5, 3, 7];

/// Timeout duration for waiting for key input from the terminal.
const KEY_TIMEOUT: Duration = Duration::from_millis(50);

/// VT100 Serial Terminal Output Driver
pub struct VT100Out<'a> {
    inner: InnerSerial<'a>,
    mode: SimpleTextOutputMode,
    is_shifted_out: bool,
    charset: CharsetMode,
}

/// Character Set Mode for VT100 Terminal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum CharsetMode {
    /// Terminal supports box drawing characters via VT100 charset switching.
    AsciiBoxChar,
    /// Terminal supports pure ASCII characters only.
    AsciiFallback,
    /// Terminal supports UTF-8.
    UTF8,
}

impl<'a> VT100Out<'a> {
    /// Create a new instance with the default charset mode.
    #[inline]
    pub const fn new(inner: &'a mut dyn SerialIo) -> Self {
        Self {
            inner: InnerSerial::new(inner),
            mode: SimpleTextOutputMode::new(),
            is_shifted_out: false,
            charset: CharsetMode::UTF8,
        }
    }

    /// Create a new instance with the specified charset mode.
    #[inline]
    pub const fn with_charset(inner: &'a mut dyn SerialIo, charset: CharsetMode) -> Self {
        Self {
            inner: InnerSerial::new(inner),
            mode: SimpleTextOutputMode::new(),
            is_shifted_out: false,
            charset,
        }
    }

    /// Wait for a byte to be available and read it.
    pub fn wait_byte(&mut self) -> Option<u8> {
        {
            let mut sio_event = self.inner.0.event_for_read();
            let mut timer = Event::with_timeout(KEY_TIMEOUT);
            System::wait_for_events(&mut [&mut sio_event, &mut timer]);
        }
        self.inner.0.read_byte()
    }
}

impl VT100Out<'_> {
    /// Reset the terminal and set the charset mode.
    #[inline]
    pub fn reset_with_charset(&mut self, charset: CharsetMode) {
        self.charset = charset;
        (self as &mut dyn SimpleTextOutput).reset();
    }

    /// Wait for a response of the form `ESC [ rows ; cols response_type` and parse the coordinates.
    pub fn wait_coords(&mut self, response_type: u8) -> Option<(u8, u8)> {
        let b = self.wait_byte()?;
        if b != 0x1b {
            return None;
        }
        let b = self.wait_byte()?;
        if b != b'[' {
            return None;
        }
        let mut buf = [0u8; 16];
        let mut i = 0;
        while i < buf.len() {
            let b = self.wait_byte()?;
            buf[i] = b;
            i += 1;
            if b == response_type {
                break;
            }
        }
        if i < 4 || buf[i - 1] != response_type {
            return None;
        }
        let mut semicolon_index = None;
        for j in 0..i - 1 {
            if buf[j] == b';' {
                semicolon_index = Some(j);
                break;
            }
        }
        let semicolon_index = semicolon_index?;
        let rows = core::str::from_utf8(&buf[0..semicolon_index]).ok()?;
        let cols = core::str::from_utf8(&buf[semicolon_index + 1..i - 1]).ok()?;
        let rows: u8 = rows.parse().ok()?;
        let cols: u8 = cols.parse().ok()?;
        Some((cols, rows))
    }

    /// Get the terminal size by querying the terminal.
    pub fn get_terminal_size(&mut self) -> Option<(u8, u8)> {
        self.inner.0.flush_input();
        let _ = self.inner.write_str("\x1b[18t");
        self.wait_coords(b't')
    }

    /// Get the current cursor position by querying the terminal.
    pub fn get_cursor_position(&mut self) -> Option<(u8, u8)> {
        self.inner.0.flush_input();
        let _ = self.inner.write_str("\x1b[6n");
        self.wait_coords(b'R').map(|(col, row)| (col - 1, row - 1))
    }

    /// Update the cursor position in the mode by querying the terminal.
    #[inline]
    pub fn update_cursor_position(&mut self) {
        if let Some((col, row)) = self.get_cursor_position() {
            self.mode.cursor_column = col;
            self.mode.cursor_row = row;
        }
    }
}

impl Write for VT100Out<'_> {
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

impl Write for InnerSerial<'_> {
    #[inline]
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.0.write_bytes(s.as_bytes());
        Ok(())
    }
}

impl SimpleTextOutput for VT100Out<'_> {
    fn reset(&mut self) {
        // reset inner tty
        self.inner.0.reset();

        // reset terminal
        let _ = self.inner.write_str("\x1bc");

        // get terminal size
        if let Some((col, row)) = self.get_terminal_size() {
            self.mode.columns = col;
            self.mode.rows = row;
        } else {
            // fallback: move cursor to bottom-right and read position
            let _ = self.inner.write_str("\x1b[255;255H");
            if let Some((col, row)) = self.get_cursor_position() {
                self.mode.columns = col.saturating_add(1);
                self.mode.rows = row.saturating_add(1);
            }
        }

        // reset attributes
        self.set_attribute(0);

        // enable box drawing charset
        // if matches!(self.charset, CharsetMode::AsciiBoxChar) {
        let _ = self.inner.write_str("\x1b(B\x1b)0\x0f");
        self.is_shifted_out = false;
        // }

        self.clear_screen();
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

/// VT100 Terminal Input/Output Driver
pub struct VT100<'a> {
    inner: VT100Out<'a>,
    key_buffer: heapless::Vec<NonZeroInputKey, 16>,
}

impl<'a> VT100<'a> {
    /// Create a new instance with the default charset mode.
    #[inline]
    pub const fn new(inner: &'a mut dyn SerialIo) -> Self {
        Self {
            inner: VT100Out::new(inner),
            key_buffer: heapless::Vec::new(),
        }
    }

    /// Create a new instance with the specified charset mode.
    #[inline]
    pub const fn with_charset(inner: &'a mut dyn SerialIo, charset: CharsetMode) -> Self {
        Self {
            inner: VT100Out::with_charset(inner, charset),
            key_buffer: heapless::Vec::new(),
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

impl VT100<'_> {
    /// Refill the key buffer by reading from the terminal.
    fn refill(&mut self) {
        if !self.key_buffer.is_empty() {
            return;
        }

        match self.inner.inner.0.read_byte() {
            None => {}
            Some(b'\x1b') => {
                match self.inner.wait_byte() {
                    None | Some(b'\x1b') => {
                        // treat as ESC key
                        let key_stroke = KeyStroke::from_usage(Usage::KEY_ESCAPE);
                        let key = InputKey::new(key_stroke, '\x1b' as u16);
                        NonZeroInputKey::from_input_key(key).map(|v| {
                            let _ = self.key_buffer.push(v);
                        });
                    }
                    Some(ch) => {
                        // decode escape sequences
                        let key_stroke = match ch {
                            b'A' => Some(KeyStroke::from_usage(Usage::KEY_UP_ARROW)),
                            b'B' => Some(KeyStroke::from_usage(Usage::KEY_DOWN_ARROW)),
                            b'C' => Some(KeyStroke::from_usage(Usage::KEY_RIGHT_ARROW)),
                            b'D' => Some(KeyStroke::from_usage(Usage::KEY_LEFT_ARROW)),
                            b'O' => {
                                // function keys F1-F4
                                match self.inner.wait_byte() {
                                    Some(b'P') => Some(KeyStroke::from_usage(Usage::KEY_F1)),
                                    Some(b'Q') => Some(KeyStroke::from_usage(Usage::KEY_F2)),
                                    Some(b'R') => Some(KeyStroke::from_usage(Usage::KEY_F3)),
                                    Some(b'S') => Some(KeyStroke::from_usage(Usage::KEY_F4)),
                                    _ => None,
                                }
                            }
                            b'[' => {
                                // extended escape sequences
                                let mut seq_buf = [0u8; 16];
                                let mut i = 0;
                                while i < seq_buf.len() {
                                    match self.inner.wait_byte() {
                                        Some(b) => {
                                            seq_buf[i] = b;
                                            i += 1;
                                            if (b'A'..=b'Z').contains(&b)
                                                || (b'a'..=b'z').contains(&b)
                                            {
                                                break;
                                            }
                                        }
                                        None => break,
                                    }
                                }
                                if i == 0 {
                                    None
                                } else {
                                    let last_byte = seq_buf[i - 1];
                                    match last_byte {
                                        b'A' => Some(KeyStroke::from_usage(Usage::KEY_UP_ARROW)),
                                        b'B' => Some(KeyStroke::from_usage(Usage::KEY_DOWN_ARROW)),
                                        b'C' => Some(KeyStroke::from_usage(Usage::KEY_RIGHT_ARROW)),
                                        b'D' => Some(KeyStroke::from_usage(Usage::KEY_LEFT_ARROW)),
                                        b'H' => Some(KeyStroke::from_usage(Usage::KEY_HOME)),
                                        b'F' => Some(KeyStroke::from_usage(Usage::KEY_END)),
                                        _ => None,
                                    }
                                }
                            }
                            _ => None,
                        };

                        if let Some(key_stroke) = key_stroke {
                            let key = InputKey::new(key_stroke, 0);
                            NonZeroInputKey::from_input_key(key).map(|v| {
                                let _ = self.key_buffer.push(v);
                            });
                        } else {
                            self.push_char(ch as char);
                        }
                    }
                }
            }
            Some(ch) => {
                self.push_char(ch as char);
            }
        }
    }

    /// Push a character into the key buffer.
    fn push_char(&mut self, ch: char) {
        let key_stroke = HidManager::estimate_key_stroke_from_char(ch)
            .unwrap_or(KeyStroke::new(Usage::ERR_UNDEFINED, Modifier::empty()));
        let key = InputKey::new(key_stroke, ch as u16);
        NonZeroInputKey::from_input_key(key).map(|v| {
            let _ = self.key_buffer.push(v);
        });
    }
}

impl SimpleTextInput for VT100<'_> {
    fn reset(&mut self) {
        self.key_buffer.clear();
        self.inner.inner.0.reset();
    }

    fn is_ready(&mut self) -> bool {
        self.refill();
        !self.key_buffer.is_empty()
    }

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        self.is_ready().then(|| self.key_buffer.remove(0))
    }
}

/// Inner Serial Device Wrapper
struct InnerSerial<'a>(&'a mut dyn SerialIo);

impl<'a> InnerSerial<'a> {
    #[inline]
    pub const fn new(inner: &'a mut dyn SerialIo) -> Self {
        Self(inner)
    }
}

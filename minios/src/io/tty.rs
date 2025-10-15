//! Simple Console I/O

pub mod fbcon;
pub mod null;
pub mod vt100;

use core::num::NonZero;

pub trait SimpleTextInput {
    fn reset(&mut self);

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey>;

    fn wait_for_key(&mut self, timeout: usize) -> Option<NonZeroInputKey> {
        loop {
            if let Some(key) = self.read_key_stroke() {
                return Some(key);
            }
            if timeout == 0 {
                return None;
            } else {
                // TODO:
            }
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputKey {
    pub scan_code: u16,
    pub unicode_char: u16,
}

pub trait SimpleTextOutput: core::fmt::Write {
    fn reset(&mut self);

    fn set_attribute(&mut self, attribute: u8);

    fn clear_screen(&mut self);

    fn set_cursor_position(&mut self, col: u32, row: u32);

    fn enable_cursor(&mut self, visible: bool) -> bool;

    fn current_mode(&mut self) -> SimpleTextOutputMode;
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct SimpleTextOutputMode {
    pub columns: u8,
    pub rows: u8,
    pub cursor_column: u8,
    pub cursor_row: u8,
    pub attribute: u8,
    pub cursor_visible: u8,
}

impl SimpleTextOutputMode {
    #[inline]
    pub const fn default() -> Self {
        Self {
            columns: 80,
            rows: 24,
            cursor_column: 0,
            cursor_row: 0,
            attribute: 0,
            cursor_visible: 1,
        }
    }

    #[inline]
    pub const fn is_cursor_visible(&self) -> bool {
        self.cursor_visible != 0
    }

    #[inline]
    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor_visible = visible as u8;
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonZeroInputKey(NonZero<u32>);

impl NonZeroInputKey {
    #[inline]
    pub const fn new(scan_code: u16, unicode_char: u16) -> Option<Self> {
        if scan_code == 0 {
            return None;
        }
        let raw = scan_code as u32 | (unicode_char as u32) << 16;
        Some(Self(unsafe { NonZero::new_unchecked(raw) }))
    }

    #[inline]
    pub fn get(self) -> InputKey {
        let raw = self.0.get();
        InputKey {
            scan_code: raw as u16,
            unicode_char: (raw >> 16) as u16,
        }
    }
}

impl From<InputKey> for Option<NonZeroInputKey> {
    #[inline]
    fn from(key: InputKey) -> Self {
        NonZeroInputKey::new(key.scan_code, key.unicode_char)
    }
}

pub trait SerialIo {
    fn reset(&mut self);

    fn write_byte(&mut self, byte: u8);

    fn read_byte(&mut self) -> Option<u8>;

    fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_byte(b);
        }
    }

    fn flush_input(&mut self) {
        while self.read_byte().is_some() {}
    }
}

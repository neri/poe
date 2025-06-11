//! Simple Console I/O

use core::num::NonZero;

pub trait SimpleTextInput {
    fn reset(&mut self);

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey>;
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputKey {
    pub usage: u16,
    pub unicode_char: u16,
}

pub trait SimpleTextOutput: core::fmt::Write {
    fn reset(&mut self);

    fn set_attribute(&mut self, attribute: u8);

    fn clear_screen(&mut self);

    fn set_cursor_position(&mut self, col: u32, row: u32);

    fn enable_cursor(&mut self, visible: bool) -> bool;

    fn current_mode(&self) -> SimpleTextOutputMode;
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
        let raw = scan_code as u32 | (unicode_char as u32) << 16;
        match NonZero::new(raw) {
            Some(key) => Some(Self(key)),
            None => None,
        }
    }

    #[inline]
    pub fn get(self) -> InputKey {
        let raw = self.0.get();
        InputKey {
            usage: raw as u16,
            unicode_char: (raw >> 16) as u16,
        }
    }
}

impl From<InputKey> for Option<NonZeroInputKey> {
    #[inline]
    fn from(key: InputKey) -> Self {
        NonZeroInputKey::new(key.usage, key.unicode_char)
    }
}

pub struct NullTty;

impl SimpleTextInput for NullTty {
    fn reset(&mut self) {}

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        None
    }
}

impl core::fmt::Write for NullTty {
    fn write_str(&mut self, _s: &str) -> core::fmt::Result {
        Ok(())
    }
}

impl SimpleTextOutput for NullTty {
    fn reset(&mut self) {}

    fn set_attribute(&mut self, _attribute: u8) {}

    fn clear_screen(&mut self) {}

    fn set_cursor_position(&mut self, _col: u32, _row: u32) {}

    fn enable_cursor(&mut self, _visible: bool) -> bool {
        false
    }

    fn current_mode(&self) -> SimpleTextOutputMode {
        SimpleTextOutputMode {
            columns: 80,
            rows: 25,
            cursor_column: 0,
            cursor_row: 0,
            attribute: 0,
            cursor_visible: 0,
        }
    }
}

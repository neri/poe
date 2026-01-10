//! Simple Console I/O

pub mod null;
pub mod vt100;

use crate::io::hid_mgr::{HidManager, KeyStroke};
use crate::task::event::{Event, PollResult, PollingEvent};
use crate::*;
use core::num::NonZero;
use libhid::{Modifier, Usage};

pub trait SimpleTextInput {
    fn reset(&mut self);

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey>;

    fn is_ready(&mut self) -> bool;
}

impl<'a> dyn SimpleTextInput + 'a {
    pub fn event_for_key<'b>(&'b mut self) -> Event<'b> {
        Event::with_polling(SimpleTextInputPoller(self))
    }
}

#[repr(transparent)]
struct SimpleTextInputPoller<'a>(&'a mut dyn SimpleTextInput);

impl PollingEvent for SimpleTextInputPoller<'_> {
    fn poll(&mut self) -> PollResult {
        if self.0.is_ready() {
            PollResult::Ready
        } else {
            PollResult::Pending
        }
    }
}

/// Input key structure.
///
/// `scan_code` (`usage()`) is represented as a HID Usage ID.
/// `unicode_char` is a UTF-16 code unit.
///
/// # Note
///
/// * Invalid keystroke if `usage` value is 0.
/// * If the `unicode_char` is 0, the key represents a non-character special key; refer to the `usage` value value for details in this case.
/// * When the `unicode_char` falls within the range U+0020 to U+007E, the `usage` value may vary depending on the input locale and may not match the expected value.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputKey {
    key_stroke: KeyStroke,
    unicode_char: u16,
}

impl InputKey {
    #[inline]
    pub fn new(key_stroke: KeyStroke, unicode_char: u16) -> Self {
        Self {
            key_stroke,
            unicode_char,
        }
    }

    #[inline]
    pub fn from_key_stroke(key_stroke: KeyStroke) -> Self {
        let unicode_char = HidManager::translate(key_stroke)
            .map(|c| c as u16)
            .unwrap_or(0);
        Self::new(key_stroke, unicode_char)
    }

    /// Returns the Unicode character, if possible.
    #[inline]
    pub fn unicode_char(&self) -> Option<char> {
        char::from_u32(self.unicode_char as u32)
    }

    #[inline]
    pub fn key_stroke(&self) -> KeyStroke {
        self.key_stroke
    }

    /// Checks if the key is a special key (non-character).
    #[inline]
    pub const fn is_special_key(&self) -> bool {
        self.unicode_char == 0
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonZeroInputKey(NonZero<u32>);

impl NonZeroInputKey {
    #[inline]
    pub fn from_input_key(key: InputKey) -> Option<Self> {
        if key.key_stroke.usage == Usage::NONE {
            return None;
        }
        let raw_value = key.key_stroke.usage.0 as u32
            | ((key.key_stroke.modifier.bits() as u32) << 8)
            | ((key.unicode_char as u32) << 16);
        // SAFETY: non-zero checked above
        Some(Self(unsafe { NonZero::new_unchecked(raw_value) }))
    }

    #[inline]
    pub fn get(self) -> InputKey {
        let raw_value = self.0.get();
        InputKey {
            key_stroke: KeyStroke {
                usage: Usage(raw_value as u8),
                modifier: Modifier::from_bits_retain((raw_value >> 8) as u8),
            },
            unicode_char: (raw_value >> 16) as u16,
        }
    }
}

impl From<InputKey> for Option<NonZeroInputKey> {
    #[inline]
    fn from(key: InputKey) -> Self {
        NonZeroInputKey::from_input_key(key)
    }
}

pub trait SimpleTextOutput: core::fmt::Write {
    fn reset(&mut self);

    fn set_attribute(&mut self, attribute: u8);

    fn clear_screen(&mut self);

    fn set_cursor_position(&mut self, col: u32, row: u32);

    fn enable_cursor(&mut self, visible: bool) -> bool;

    fn current_mode(&mut self) -> SimpleTextOutputMode;
}

impl tui::TuiDrawTarget for dyn SimpleTextOutput {
    fn draw(&mut self, origin: tui::coord::Point, text: &str, attr: tui::color::TuiAttribute) {
        self.set_cursor_position(origin.x as u32, origin.y as u32);
        self.set_attribute(attr.0);
        let _ = self.write_str(text);
    }
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
    pub const fn new() -> Self {
        Self::from_dims(80, 24)
    }

    #[inline]
    pub const fn from_dims(columns: u8, rows: u8) -> Self {
        Self {
            columns,
            rows,
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

pub trait SerialIo {
    fn reset(&mut self);

    fn write_byte(&mut self, byte: u8);

    fn read_byte(&mut self) -> Option<u8>;

    fn is_ready_to_read(&mut self) -> bool;

    fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_byte(b);
        }
    }

    fn flush_input(&mut self) {
        while self.read_byte().is_some() {}
    }
}

impl<'a> dyn SerialIo + 'a {
    pub fn event_for_read<'b>(&'b mut self) -> Event<'b> {
        Event::with_polling(SerialPoller(self))
    }
}

#[repr(transparent)]
struct SerialPoller<'a>(&'a mut dyn SerialIo);

impl PollingEvent for SerialPoller<'_> {
    fn poll(&mut self) -> PollResult {
        if self.0.is_ready_to_read() {
            PollResult::Ready
        } else {
            PollResult::Pending
        }
    }
}

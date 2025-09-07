//! Null TTY implementation

use super::*;

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

    fn current_mode(&mut self) -> SimpleTextOutputMode {
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

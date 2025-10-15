//! Framebuffer console implementation

use super::*;
use crate::*;

#[allow(unused)]
pub struct FbCon {
    mode: SimpleTextOutputMode,
}

impl FbCon {
    fn flip_cursor(&mut self, _col: u8, _row: u8) {
        todo!()
    }
}

// impl SimpleTextInput for FbCon {
//     fn reset(&mut self) {}

//     fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
//         None
//     }
// }

impl core::fmt::Write for FbCon {
    fn write_str(&mut self, _s: &str) -> core::fmt::Result {
        Ok(())
    }
}

impl SimpleTextOutput for FbCon {
    fn reset(&mut self) {
        self.clear_screen();
    }

    fn set_attribute(&mut self, attribute: u8) {
        let attribute = if attribute == 0 {
            System::DEFAULT_STDOUT_ATTRIBUTE
        } else {
            attribute
        };
        self.mode.attribute = attribute;
    }

    fn clear_screen(&mut self) {
        let old_cursor_visible = self.enable_cursor(false);

        self.mode.cursor_column = 0;
        self.mode.cursor_row = 0;
        if old_cursor_visible {
            self.enable_cursor(old_cursor_visible);
        }
    }

    fn set_cursor_position(&mut self, col: u32, row: u32) {
        let old_cursor_visible = self.enable_cursor(false);
        self.mode.cursor_column = (self.mode.columns as u32).min(col) as u8;
        self.mode.cursor_row = (self.mode.rows as u32).min(row) as u8;
        if old_cursor_visible {
            self.flip_cursor(self.mode.cursor_column, self.mode.cursor_row);
        }
    }

    fn enable_cursor(&mut self, visible: bool) -> bool {
        let old_value = self.mode.is_cursor_visible();
        if visible != old_value {
            self.flip_cursor(self.mode.cursor_column, self.mode.cursor_row);
        }
        self.mode.set_cursor_visible(visible);
        old_value
    }

    fn current_mode(&mut self) -> SimpleTextOutputMode {
        self.mode.clone()
    }
}

//! Text User Interface

use crate::*;
// use alloc::format;

pub struct Tui;

impl Tui {
    pub fn draw_title(title: &str) {
        let stdout = System::stdout();
        let mode = stdout.current_mode();
        // stdout.reset();

        stdout.set_cursor_position(0, 0);
        stdout.set_attribute(0xf0);
        for _ in 0..mode.columns {
            let _ = stdout.write_char(' ');
        }

        stdout.set_cursor_position(mode.columns as u32 / 2 - (title.len() as u32) / 2, 0);
        println!("{}", title);
        stdout.set_attribute(0);
    }
}

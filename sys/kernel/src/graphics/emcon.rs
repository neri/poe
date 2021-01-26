// Emergency debugging console

use crate::fonts::*;
use crate::graphics::bitmap::*;
use crate::graphics::color::*;
use crate::graphics::coords::*;
use crate::system::*;
use core::fmt;

pub struct EmConsole {
    x: usize,
    y: usize,
    fg_color: IndexedColor,
    bg_color: IndexedColor,
}

impl EmConsole {
    pub const fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            fg_color: IndexedColor::WHITE,
            bg_color: IndexedColor::BLUE,
        }
    }

    pub fn write_char(&mut self, c: char) {
        // let font = FontManager::fixed_system_font();
        let font = FontManager::fixed_small_font();
        let font_size = Size::new(font.width(), font.line_height());
        let bitmap = System::main_screen();

        // check bounds
        let cols = bitmap.width() / font_size.width() as usize;
        let rows = bitmap.height() / font_size.height() as usize;
        if self.x >= cols {
            self.x = 0;
            self.y += 1;
        }
        if self.y >= rows {
            self.y = rows - 1;
            let sh = font_size.height() * self.y as isize;
            let mut rect = bitmap.bounds();
            rect.origin.y += font_size.height();
            rect.size.height = sh;
            bitmap.blt(&bitmap.clone(), Point::new(0, 0), rect, BltOption::empty());
            bitmap.fill_rect(
                Rect::new(0, sh, rect.width(), font_size.height()),
                self.bg_color,
            );
        }

        match c {
            '\r' => {
                self.x = 0;
            }
            '\n' => {
                self.x = 0;
                self.y += 1;
            }
            _ => {
                let origin = Point::new(
                    self.x as isize * font_size.width,
                    self.y as isize * font_size.height,
                );
                bitmap.fill_rect(
                    Rect {
                        origin,
                        size: font_size,
                    },
                    self.bg_color,
                );
                font.write_char(c, bitmap, origin, self.fg_color);

                self.x += 1;
            }
        }
    }
}

impl fmt::Write for EmConsole {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            self.write_char(c);
        }
        Ok(())
    }
}

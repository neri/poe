//! Framebuffer console implementation

use super::*;
use crate::io::graphics::color::IndexedColor;
use crate::io::graphics::display::FbDisplay8;
use crate::*;
use embedded_graphics::mono_font::{MonoFont, MonoTextStyle};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::Baseline;
use embedded_graphics::text::renderer::{CharacterStyle, TextRenderer};

pub struct FbCon {
    fb: FbDisplay8,
    font: MonoFont<'static>,
    mode: SimpleTextOutputMode,
    font_width: usize,
    font_height: usize,
    fg_color: IndexedColor,
    bg_color: IndexedColor,
}

impl FbCon {
    pub fn new(fb: FbDisplay8, font: MonoFont<'static>) -> Self {
        let char_size = font.character_size;
        let display_width = fb.bounding_box().size.width as usize;
        let display_height = fb.bounding_box().size.height as usize;
        let font_width = char_size.width as usize + font.character_spacing as usize;
        let font_height = char_size.height as usize;

        let cols = (display_width / font_width).min(255) as u8;
        let rows = (display_height / font_height).min(255) as u8;

        Self {
            fb,
            font,
            mode: SimpleTextOutputMode::from_dims(cols, rows),
            font_width,
            font_height,
            fg_color: IndexedColor::BLACK,
            bg_color: IndexedColor::BLACK,
        }
    }

    #[inline]
    pub fn current_fb(&mut self) -> &mut FbDisplay8 {
        &mut self.fb
    }

    fn draw_cursor(&mut self, col: u8, row: u8, state: bool) {
        self.fb
            .fill_solid(
                &Rectangle::new(
                    Point::new(
                        (col as usize * self.font_width) as i32,
                        (row as usize * self.font_height + self.font_height - 1) as i32,
                    ),
                    Size::new(self.font_width as u32, 1),
                ),
                if state { self.fg_color } else { self.bg_color },
            )
            .unwrap();
    }

    fn adjust_coords(&mut self, col: u8, row: u8, wrap_around: bool) -> Option<(u8, u8)> {
        if row < self.mode.rows && (!wrap_around || col < self.mode.columns) {
            return None;
        }

        let mut col = col;
        let mut row = row;

        if wrap_around {
            while col >= self.mode.columns {
                col -= self.mode.columns;
                row += 1;
            }
        }

        while row >= self.mode.rows {
            // TODO: scroll
            row -= 1;
        }

        Some((col, row))
    }
}

impl core::fmt::Write for FbCon {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let old_cursor_visible = self.enable_cursor(false);
        let mut col = self.mode.cursor_column;
        let mut row = self.mode.cursor_row;

        for ch in s.chars() {
            match ch {
                '\n' => {
                    col = 0;
                    row += 1;
                    if let Some((new_col, new_row)) = self.adjust_coords(col, row, false) {
                        col = new_col;
                        row = new_row;
                    }
                }
                '\r' => {
                    col = 0;
                }
                '\x08' => {
                    if col > 0 {
                        col -= 1;
                    }
                }
                _ => {
                    let ch = if ch >= ' ' && ch < '\x7F' {
                        ch as u8
                    } else {
                        b'?' as u8
                    };

                    if let Some((new_col, new_row)) = self.adjust_coords(col, row, true) {
                        col = new_col;
                        row = new_row;
                    }

                    let mut mono_style = MonoTextStyle::new(&self.font, self.fg_color);
                    mono_style.set_background_color(self.bg_color.into());

                    let s = [ch];
                    let s = unsafe { core::str::from_utf8_unchecked(&s) };
                    let position = Point::new(
                        (col as usize * self.font_width) as i32,
                        (row as usize * self.font_height) as i32,
                    );
                    mono_style
                        .draw_string(s, position, Baseline::Top, &mut self.fb)
                        .unwrap();

                    col += 1;
                }
            }
        }

        if let Some((new_col, new_row)) = self.adjust_coords(col, row, false) {
            col = new_col;
            row = new_row;
        }
        self.mode.cursor_column = col;
        self.mode.cursor_row = row;
        if old_cursor_visible {
            self.enable_cursor(old_cursor_visible);
        }
        Ok(())
    }
}

impl SimpleTextOutput for FbCon {
    fn reset(&mut self) {
        self.set_attribute(0);
        self.mode.set_cursor_visible(false);
        self.clear_screen();
    }

    fn set_attribute(&mut self, attribute: u8) {
        let attribute = if attribute == 0 {
            System::DEFAULT_STDOUT_ATTRIBUTE
        } else {
            attribute
        };
        self.mode.attribute = attribute;
        self.fg_color = IndexedColor(self.mode.attribute & 0x0F);
        self.bg_color = IndexedColor((self.mode.attribute & 0xF0) >> 4);
    }

    fn clear_screen(&mut self) {
        let old_cursor_visible = self.enable_cursor(false);

        self.fb
            .fill_solid(
                &Rectangle::new(
                    Point::zero(),
                    Size::new(
                        self.mode.columns as u32 * self.font_width as u32,
                        self.mode.rows as u32 * self.font_height as u32,
                    ),
                ),
                self.bg_color,
            )
            .unwrap();

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
            self.draw_cursor(
                self.mode.cursor_column,
                self.mode.cursor_row,
                old_cursor_visible,
            );
        }
    }

    fn enable_cursor(&mut self, visible: bool) -> bool {
        let old_value = self.mode.is_cursor_visible();
        if visible != old_value {
            self.draw_cursor(self.mode.cursor_column, self.mode.cursor_row, visible);
        }
        self.mode.set_cursor_visible(visible);
        old_value
    }

    fn current_mode(&mut self) -> SimpleTextOutputMode {
        self.mode.clone()
    }
}

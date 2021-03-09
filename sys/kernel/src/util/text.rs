// Text Processing

use crate::fonts::*;
use crate::graphics::bitmap::*;
use crate::graphics::color::*;
use crate::graphics::coords::*;
use alloc::vec::Vec;
use core::num::NonZeroUsize;

pub struct TextProcessing {
    //
}

pub enum LineBreakMode {
    CharWrapping,
    WordWrapping,
    TrancatingTail,
}

impl Default for LineBreakMode {
    fn default() -> Self {
        Self::CharWrapping
    }
}

pub enum TextAlignment {
    Left,
    Center,
    Right,
    Leading,
    Trailing,
}

impl Default for TextAlignment {
    fn default() -> Self {
        Self::Leading
    }
}

pub enum VerticalAlignment {
    Top,
    Bottom,
    Center,
}

impl Default for VerticalAlignment {
    fn default() -> Self {
        Self::Top
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LineStatus {
    pub start_position: usize,
    pub end_position: usize,
    pub width: isize,
    pub height: isize,
}

impl LineStatus {
    const fn empty() -> Self {
        Self {
            start_position: 0,
            end_position: 0,
            width: 0,
            height: 0,
        }
    }
}

impl TextProcessing {
    pub fn line_statuses(
        font: &FixedFontDriver,
        s: &str,
        size: Size,
        max_lines: usize,
        line_break: LineBreakMode,
    ) -> Vec<LineStatus> {
        let max_lines = NonZeroUsize::new(max_lines)
            .map(|v| v.get())
            .unwrap_or(usize::MAX);
        let limit_max_lines = 64;
        let mut vec = Vec::with_capacity(usize::min(max_lines, limit_max_lines));

        // TODO: Line Breaking
        let _ = line_break;

        let mut current_line = LineStatus::empty();
        current_line.height = font.line_height();
        let mut current_height = current_line.height;
        for (index, c) in s.chars().enumerate() {
            match c {
                '\n' => {
                    current_line.end_position = index;
                    current_height += current_line.height;
                    vec.push(current_line);
                    current_line = LineStatus::empty();
                    if vec.len() >= max_lines || current_height >= size.height() {
                        break;
                    }
                    current_line.start_position = index + 1;
                    current_line.height = font.line_height();
                }
                _ => {
                    current_line.end_position = index;
                    let current_width = font.width_of(c);
                    let new_width = current_line.width + current_width;
                    if current_line.width > 0 && new_width > size.width {
                        current_height += current_line.height;
                        vec.push(current_line);
                        current_line = LineStatus::empty();
                        if vec.len() >= max_lines || current_height >= size.height() {
                            break;
                        }
                        current_line.start_position = index;
                        current_line.width = current_width;
                        current_line.height = font.line_height();
                    } else {
                        current_line.width = new_width;
                    }
                }
            }
        }
        if vec.len() < max_lines && current_line.width > 0 {
            current_line.end_position += 1;
            vec.push(current_line);
        }

        vec
    }

    pub fn bounding_size(
        font: &FixedFontDriver,
        s: &str,
        size: Size,
        max_lines: usize,
        line_break: LineBreakMode,
    ) -> Size {
        let lines = Self::line_statuses(font, s, size, max_lines, line_break);
        Size::new(
            lines.iter().fold(0, |v, i| isize::max(v, i.width)),
            lines.iter().fold(0, |v, i| v + i.height),
        )
    }

    /// Write string to bitmap
    pub fn write_str(
        to: &mut Bitmap,
        s: &str,
        font: &FixedFontDriver,
        origin: Point,
        color: AmbiguousColor,
    ) {
        Self::draw_text(
            to,
            s,
            font,
            Coordinates::new(
                origin.x,
                origin.y,
                to.width() as isize,
                to.height() as isize,
            )
            .into(),
            color,
            1,
            LineBreakMode::default(),
            TextAlignment::default(),
            VerticalAlignment::default(),
        )
    }

    /// Write text to bitmap
    pub fn draw_text(
        to: &mut Bitmap,
        s: &str,
        font: &FixedFontDriver,
        rect: Rect,
        color: AmbiguousColor,
        max_lines: usize,
        line_break: LineBreakMode,
        align: TextAlignment,
        valign: VerticalAlignment,
    ) {
        let coords = match Coordinates::from_rect(rect) {
            Ok(v) => v,
            Err(_) => return,
        };

        let lines = Self::line_statuses(font, s, rect.size(), max_lines, line_break);
        let mut chars = s.chars();
        let mut cursor = Point::default();
        let mut prev_position = 0;

        let perferred_height = lines.iter().fold(0, |v, i| v + i.height);
        cursor.y = match valign {
            VerticalAlignment::Top => coords.top,
            VerticalAlignment::Center => isize::max(
                coords.top,
                coords.top + (coords.bottom - perferred_height) / 2,
            ),
            VerticalAlignment::Bottom => isize::max(coords.top, coords.bottom - perferred_height),
        };
        for line in lines {
            for _ in prev_position..line.start_position {
                let _ = chars.next();
            }

            if line.start_position < line.end_position {
                cursor.x = match align {
                    TextAlignment::Leading | TextAlignment::Left => coords.left,
                    TextAlignment::Trailing | TextAlignment::Right => coords.right - line.width,
                    TextAlignment::Center => coords.left + (rect.width() - line.width) / 2,
                };
                for _ in line.start_position..line.end_position {
                    let c = chars.next().unwrap();
                    font.write_char(c, to, cursor, color);
                    cursor.x += font.width_of(c);
                }
            }

            prev_position = line.end_position;
            cursor.y += line.height;
        }
    }
}

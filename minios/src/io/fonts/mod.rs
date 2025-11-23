pub use simple_font::*;

/// Selects a preferred font based on the given width and height.
#[inline]
pub fn preferred_font_for(width: u32, height: u32) -> SimpleFont<'static> {
    if width >= 800 && height >= 600 {
        FONT_1020
    } else if width >= 640 && height >= 400 {
        FONT_DEFAULT
    } else {
        FONT_SMALL
    }
}

pub const FONT_DEFAULT: SimpleFont<'static> = include_font!("./megh0816.png", 8, 16);

pub const FONT_SMALL: SimpleFont<'static> = include_font!("./megh0608.png", 6, 8);

pub const FONT_1020: SimpleFont<'static> = include_font!("./megb1020.png", 10, 20);

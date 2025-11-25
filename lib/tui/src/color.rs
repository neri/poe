//! Color and attribute definitions

/// Text attribute (foreground and background color)
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct TuiAttribute(pub u8);

/// Common console colors
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum TuiColor {
    #[default]
    Black = 0,
    Blue,
    Green,
    Cyan,
    Red,
    Magenta,
    Brown,
    LightGray,
    DarkGray,
    LightBlue,
    LightGreen,
    LightCyan,
    LightRed,
    LightMagenta,
    Yellow,
    White,
}

impl TuiAttribute {
    #[inline]
    pub const fn new(fg: TuiColor, bg: TuiColor) -> Self {
        let val = (fg as u8) | ((bg as u8) << 4);
        Self(val)
    }

    #[inline]
    pub const fn fg(fg: TuiColor) -> Self {
        Self(fg as u8)
    }

    #[inline]
    pub const fn bg(bg: TuiColor) -> Self {
        Self((bg as u8) << 4)
    }

    #[inline]
    pub fn reverse(&mut self) {
        *self = self.reversed();
    }

    #[inline]
    pub const fn reversed(self) -> Self {
        let fg = self.fg_color();
        let bg = self.bg_color();
        TuiAttribute::new(bg, fg)
    }

    #[inline]
    pub const fn fg_color(self) -> TuiColor {
        // SAFETY: All values are within the valid range
        unsafe { core::mem::transmute(self.0 & 0x0F) }
    }

    #[inline]
    pub const fn bg_color(self) -> TuiColor {
        // SAFETY: All values are within the valid range
        unsafe { core::mem::transmute((self.0 >> 4) & 0x0F) }
    }
}

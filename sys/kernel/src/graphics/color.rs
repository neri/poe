// Colors

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IndexedColor(pub u8);

impl IndexedColor {
    pub const BLACK: Self = Self(0);
    pub const BLUE: Self = Self(1);
    pub const GREEN: Self = Self(2);
    pub const CYAN: Self = Self(3);
    pub const RED: Self = Self(4);
    pub const MAGENTA: Self = Self(5);
    pub const BROWN: Self = Self(6);
    pub const LIGHT_GRAY: Self = Self(7);
    pub const DARK_GRAY: Self = Self(8);
    pub const LIGHT_BLUE: Self = Self(9);
    pub const LIGHT_GREEN: Self = Self(10);
    pub const LIGHT_CYAN: Self = Self(11);
    pub const LIGHT_RED: Self = Self(12);
    pub const LIGHT_MAGENTA: Self = Self(13);
    pub const YELLOW: Self = Self(14);
    pub const WHITE: Self = Self(15);
}

impl From<u8> for IndexedColor {
    fn from(val: u8) -> Self {
        Self(val)
    }
}

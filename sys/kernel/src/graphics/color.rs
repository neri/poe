// Colors

use core::mem::transmute;

pub trait ColorTrait: Sized + Copy + Clone + PartialEq + Eq {}

impl ColorTrait for IndexedColor {}
impl ColorTrait for TrueColor {}
impl ColorTrait for AmbiguousColor {}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexedColor(pub u8);

const SYSTEM_COLOR_PALETTE: [u32; 256] = [
    0x212121, 0x0D47A1, 0x1B5E20, 0x006064, 0xb71c1c, 0x4A148C, 0x795548, 0x9E9E9E, 0x616161,
    0x2196F3, 0x4CAF50, 0x00BCD4, 0xf44336, 0x9C27B0, 0xFFEB3B, 0xFFFFFF, 0x000000, 0x330000,
    0x660000, 0x990000, 0xCC0000, 0xFF0000, 0x003300, 0x333300, 0x663300, 0x993300, 0xCC3300,
    0xFF3300, 0x006600, 0x336600, 0x666600, 0x996600, 0xCC6600, 0xFF6600, 0x009900, 0x339900,
    0x669900, 0x999900, 0xCC9900, 0xFF9900, 0x00CC00, 0x33CC00, 0x66CC00, 0x99CC00, 0xCCCC00,
    0xFFCC00, 0x00FF00, 0x33FF00, 0x66FF00, 0x99FF00, 0xCCFF00, 0xFFFF00, 0x000033, 0x330033,
    0x660033, 0x990033, 0xCC0033, 0xFF0033, 0x003333, 0x333333, 0x663333, 0x993333, 0xCC3333,
    0xFF3333, 0x006633, 0x336633, 0x666633, 0x996633, 0xCC6633, 0xFF6633, 0x009933, 0x339933,
    0x669933, 0x999933, 0xCC9933, 0xFF9933, 0x00CC33, 0x33CC33, 0x66CC33, 0x99CC33, 0xCCCC33,
    0xFFCC33, 0x00FF33, 0x33FF33, 0x66FF33, 0x99FF33, 0xCCFF33, 0xFFFF33, 0x000066, 0x330066,
    0x660066, 0x990066, 0xCC0066, 0xFF0066, 0x003366, 0x333366, 0x663366, 0x993366, 0xCC3366,
    0xFF3366, 0x006666, 0x336666, 0x666666, 0x996666, 0xCC6666, 0xFF6666, 0x009966, 0x339966,
    0x669966, 0x999966, 0xCC9966, 0xFF9966, 0x00CC66, 0x33CC66, 0x66CC66, 0x99CC66, 0xCCCC66,
    0xFFCC66, 0x00FF66, 0x33FF66, 0x66FF66, 0x99FF66, 0xCCFF66, 0xFFFF66, 0x000099, 0x330099,
    0x660099, 0x990099, 0xCC0099, 0xFF0099, 0x003399, 0x333399, 0x663399, 0x993399, 0xCC3399,
    0xFF3399, 0x006699, 0x336699, 0x666699, 0x996699, 0xCC6699, 0xFF6699, 0x009999, 0x339999,
    0x669999, 0x999999, 0xCC9999, 0xFF9999, 0x00CC99, 0x33CC99, 0x66CC99, 0x99CC99, 0xCCCC99,
    0xFFCC99, 0x00FF99, 0x33FF99, 0x66FF99, 0x99FF99, 0xCCFF99, 0xFFFF99, 0x0000CC, 0x3300CC,
    0x6600CC, 0x9900CC, 0xCC00CC, 0xFF00CC, 0x0033CC, 0x3333CC, 0x6633CC, 0x9933CC, 0xCC33CC,
    0xFF33CC, 0x0066CC, 0x3366CC, 0x6666CC, 0x9966CC, 0xCC66CC, 0xFF66CC, 0x0099CC, 0x3399CC,
    0x6699CC, 0x9999CC, 0xCC99CC, 0xFF99CC, 0x00CCCC, 0x33CCCC, 0x66CCCC, 0x99CCCC, 0xCCCCCC,
    0xFFCCCC, 0x00FFCC, 0x33FFCC, 0x66FFCC, 0x99FFCC, 0xCCFFCC, 0xFFFFCC, 0x0000FF, 0x3300FF,
    0x6600FF, 0x9900FF, 0xCC00FF, 0xFF00FF, 0x0033FF, 0x3333FF, 0x6633FF, 0x9933FF, 0xCC33FF,
    0xFF33FF, 0x0066FF, 0x3366FF, 0x6666FF, 0x9966FF, 0xCC66FF, 0xFF66FF, 0x0099FF, 0x3399FF,
    0x6699FF, 0x9999FF, 0xCC99FF, 0xFF99FF, 0x00CCFF, 0x33CCFF, 0x66CCFF, 0x99CCFF, 0xCCCCFF,
    0xFFCCFF, 0x00FFFF, 0x33FFFF, 0x66FFFF, 0x99FFFF, 0xCCFFFF, 0xFFFFFF, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

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
    pub const DEFAULT_KEY: Self = Self(0xFF);

    pub const fn from_rgb(rgb: u32) -> Self {
        let b = (((rgb & 0xFF) + 25) / 51) as u8;
        let g = ((((rgb >> 8) & 0xFF) + 25) / 51) as u8;
        let r = ((((rgb >> 16) & 0xFF) + 25) / 51) as u8;
        Self(16 + r + g * 6 + b * 36)
    }

    pub const fn as_rgb(self) -> u32 {
        SYSTEM_COLOR_PALETTE[self.0 as usize]
    }

    pub const fn as_true_color(self) -> TrueColor {
        TrueColor::from_rgb(self.as_rgb())
    }
}

impl From<u8> for IndexedColor {
    fn from(val: u8) -> Self {
        Self(val)
    }
}

impl From<IndexedColor> for TrueColor {
    fn from(val: IndexedColor) -> Self {
        val.as_true_color()
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct TrueColor {
    argb: u32,
}

impl TrueColor {
    pub const TRANSPARENT: Self = Self::from_argb(0);
    pub const WHITE: Self = Self::from_rgb(0xFFFFFF);

    #[inline]
    pub const fn from_rgb(rgb: u32) -> Self {
        Self {
            argb: rgb | 0xFF000000,
        }
    }

    #[inline]
    pub const fn from_argb(argb: u32) -> Self {
        Self { argb }
    }

    #[inline]
    pub const fn gray(white: u8, alpha: u8) -> Self {
        Self {
            argb: white as u32 * 0x00_01_01_01 + alpha as u32 * 0x01_00_00_00,
        }
    }

    #[inline]
    pub fn components(self) -> ColorComponents {
        self.into()
    }

    #[inline]
    pub const fn rgb(self) -> u32 {
        self.argb & 0x00FFFFFF
    }

    #[inline]
    pub const fn argb(self) -> u32 {
        self.argb
    }

    #[inline]
    pub fn brightness(self) -> u8 {
        let cc = self.components();
        ((cc.r as usize * 19589 + cc.g as usize * 38444 + cc.b as usize * 7502 + 32767) >> 16) as u8
    }

    #[inline]
    pub const fn opacity(self) -> u8 {
        (self.argb >> 24) as u8
    }

    #[inline]
    pub fn set_opacity(mut self, alpha: u8) -> Self {
        let mut components = self.components();
        components.a = alpha;
        self.argb = components.into();
        self
    }

    #[inline]
    pub const fn is_opaque(self) -> bool {
        self.opacity() == 0xFF
    }

    #[inline]
    pub const fn is_transparent(self) -> bool {
        self.opacity() == 0
    }

    #[inline]
    pub fn blend_each<F>(self, rhs: Self, f: F) -> Self
    where
        F: Fn(u8, u8) -> u8,
    {
        self.components().blend_each(rhs.into(), f).into()
    }

    #[inline]
    pub fn blend_color<F1, F2>(self, rhs: Self, f_rgb: F1, f_a: F2) -> Self
    where
        F1: Fn(u8, u8) -> u8,
        F2: Fn(u8, u8) -> u8,
    {
        self.components().blend_color(rhs.into(), f_rgb, f_a).into()
    }

    #[inline]
    pub fn blend(self, other: Self) -> Self {
        let c = other.components();
        let alpha_l = c.a as usize;
        let alpha_r = 255 - alpha_l;
        c.blend_each(self.components(), |a, b| {
            ((a as usize * alpha_l + b as usize * alpha_r) / 255) as u8
        })
        .into()
    }
}

impl From<u32> for TrueColor {
    fn from(val: u32) -> Self {
        Self::from_argb(val)
    }
}

impl From<TrueColor> for IndexedColor {
    fn from(val: TrueColor) -> Self {
        Self::from_rgb(val.rgb())
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
#[cfg(target_endian = "little")]
pub struct ColorComponents {
    pub b: u8,
    pub g: u8,
    pub r: u8,
    pub a: u8,
}

impl ColorComponents {
    #[inline]
    pub fn blend_each<F>(self, rhs: Self, f: F) -> Self
    where
        F: Fn(u8, u8) -> u8,
    {
        Self {
            a: f(self.a, rhs.a),
            r: f(self.r, rhs.r),
            g: f(self.g, rhs.g),
            b: f(self.b, rhs.b),
        }
    }

    #[inline]
    pub fn blend_color<F1, F2>(self, rhs: Self, f_rgb: F1, f_a: F2) -> Self
    where
        F1: Fn(u8, u8) -> u8,
        F2: Fn(u8, u8) -> u8,
    {
        Self {
            a: f_a(self.a, rhs.a),
            r: f_rgb(self.r, rhs.r),
            g: f_rgb(self.g, rhs.g),
            b: f_rgb(self.b, rhs.b),
        }
    }

    #[inline]
    pub const fn is_opaque(self) -> bool {
        self.a == 255
    }

    #[inline]
    pub const fn is_transparent(self) -> bool {
        self.a == 0
    }
}

impl From<TrueColor> for ColorComponents {
    fn from(color: TrueColor) -> Self {
        unsafe { transmute(color) }
    }
}

impl From<ColorComponents> for TrueColor {
    fn from(components: ColorComponents) -> Self {
        unsafe { transmute(components) }
    }
}

impl Into<u32> for ColorComponents {
    fn into(self) -> u32 {
        unsafe { transmute(self) }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AmbiguousColor {
    Indexed(IndexedColor),
    Argb32(TrueColor),
}

impl Into<IndexedColor> for AmbiguousColor {
    fn into(self) -> IndexedColor {
        match self {
            AmbiguousColor::Indexed(v) => v,
            AmbiguousColor::Argb32(v) => v.into(),
        }
    }
}

impl Into<TrueColor> for AmbiguousColor {
    fn into(self) -> TrueColor {
        match self {
            AmbiguousColor::Indexed(v) => v.into(),
            AmbiguousColor::Argb32(v) => v,
        }
    }
}

impl From<IndexedColor> for AmbiguousColor {
    fn from(val: IndexedColor) -> Self {
        Self::Indexed(val)
    }
}

impl From<TrueColor> for AmbiguousColor {
    fn from(val: TrueColor) -> Self {
        Self::Argb32(val)
    }
}

// Colors

use core::mem::transmute;

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
    pub const DEFAULT_KEY: Self = Self(0xFF);

    pub const fn from_rgb(rgb: u32) -> Self {
        let b = (((rgb & 0xFF) + 25) / 51) as u8;
        let g = ((((rgb >> 8) & 0xFF) + 25) / 51) as u8;
        let r = ((((rgb >> 16) & 0xFF) + 25) / 51) as u8;
        Self(16 + r + g * 6 + b * 36)
    }
}

impl From<u8> for IndexedColor {
    fn from(val: u8) -> Self {
        Self(val)
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct TrueColor {
    argb: u32,
}

impl TrueColor {
    pub const TRANSPARENT: Self = Self::zero();
    pub const WHITE: Self = Self::from_rgb(0xFFFFFF);

    #[inline]
    pub const fn zero() -> Self {
        Self { argb: 0 }
    }

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

//! EDID (Extended Display Identification Data) parsing library

#![cfg_attr(not(test), no_std)]

pub struct Edid<'a>(&'a [u8; 128]);

impl<'a> Edid<'a> {
    #[inline]
    pub fn new(data: &'a [u8; 128]) -> Option<Self> {
        let unchecked = Self(data);
        unchecked.is_valid().then(|| unchecked)
    }

    #[inline]
    pub const unsafe fn new_unchecked(data: &'a [u8; 128]) -> Self {
        Edid(data)
    }

    #[inline]
    pub const fn as_slice(&self) -> &'a [u8; 128] {
        self.0
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        if self.0[0..8] != [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00] {
            return false;
        }
        self.0.iter().fold(0u8, |acc, &x| acc.wrapping_add(x)) == 0
    }

    #[inline]
    pub fn active_pixels(&self) -> (u16, u16) {
        let x = self.0[0x38] as u16 | ((self.0[0x3a] as u16 & 0xf0) << 4);
        let y = self.0[0x3b] as u16 | ((self.0[0x3d] as u16 & 0xf0) << 4);
        (x, y)
    }
}

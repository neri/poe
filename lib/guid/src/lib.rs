//! Globally Unique Identifier (MS-GUID)
//!
#![cfg_attr(not(test), no_std)]

extern crate alloc;
use core::{fmt, mem::transmute};

/// Globally Unique Identifier (MS-GUID)
#[repr(C, align(8))]
#[derive(Copy, Clone, Eq)]
pub struct Guid {
    data: [u8; 16],
}

impl Guid {
    pub const NULL: Self = Self::null();

    #[inline]
    pub const fn null() -> Self {
        Self { data: [0; 16] }
    }

    /// Parse a MS-GUID from string
    ///
    /// `00112233-4455-6677-8899-aabbccddeeff` -> `33 22 11 00 55 44 77 66 88 99 aa bb cc dd ee ff`
    pub const fn try_parse(s: &str) -> Result<Self, ParseError> {
        macro_rules! ctry {
            ($expr:expr) => {
                match $expr {
                    Ok(val) => val,
                    Err(err) => return Err(err),
                }
            };
        }

        const fn parse_two_hex_digits(s: &[u8], offset: usize) -> Result<u8, ParseError> {
            let left = match s[offset] {
                b'0'..=b'9' => s[offset] - b'0',
                b'a'..=b'f' => s[offset] - b'a' + 10,
                b'A'..=b'F' => s[offset] - b'A' + 10,
                _ => return Err(ParseError::InvalidDigit),
            };
            let right = match s[offset + 1] {
                b'0'..=b'9' => s[offset + 1] - b'0',
                b'a'..=b'f' => s[offset + 1] - b'a' + 10,
                b'A'..=b'F' => s[offset + 1] - b'A' + 10,
                _ => return Err(ParseError::InvalidDigit),
            };
            Ok(left * 16 + right)
        }

        if s.len() != 36 {
            return Err(ParseError::InvalidLength);
        }
        let s = s.as_bytes();
        if s[8] != b'-' || s[13] != b'-' || s[18] != b'-' || s[23] != b'-' {
            return Err(ParseError::InvalidDelimiter);
        }

        Ok(Self::from_raw([
            ctry!(parse_two_hex_digits(s, 6)),
            ctry!(parse_two_hex_digits(s, 4)),
            ctry!(parse_two_hex_digits(s, 2)),
            ctry!(parse_two_hex_digits(s, 0)),
            ctry!(parse_two_hex_digits(s, 11)),
            ctry!(parse_two_hex_digits(s, 9)),
            ctry!(parse_two_hex_digits(s, 16)),
            ctry!(parse_two_hex_digits(s, 14)),
            ctry!(parse_two_hex_digits(s, 19)),
            ctry!(parse_two_hex_digits(s, 21)),
            ctry!(parse_two_hex_digits(s, 24)),
            ctry!(parse_two_hex_digits(s, 26)),
            ctry!(parse_two_hex_digits(s, 28)),
            ctry!(parse_two_hex_digits(s, 30)),
            ctry!(parse_two_hex_digits(s, 32)),
            ctry!(parse_two_hex_digits(s, 34)),
        ]))
    }

    pub const fn parse_or_panic(s: &str) -> Self {
        match Self::try_parse(s) {
            Ok(uuid) => uuid,
            Err(ParseError::InvalidLength) => panic!("Invalid UUID length"),
            Err(ParseError::InvalidDelimiter) => panic!("Invalid UUID delimiter"),
            Err(ParseError::InvalidDigit) => panic!("Invalid UUID digit"),
        }
    }

    #[inline]
    pub const fn from_raw(data: [u8; 16]) -> Self {
        Self { data }
    }

    #[inline]
    pub const unsafe fn as_u128(&self) -> &u128 {
        unsafe { transmute(self) }
    }

    #[inline]
    pub const fn a(&self) -> u32 {
        ((self.data[3] as u32) << 24)
            + ((self.data[2] as u32) << 16)
            + ((self.data[1] as u32) << 8)
            + (self.data[0] as u32)
    }

    #[inline]
    pub const fn b(&self) -> u16 {
        ((self.data[5] as u16) << 8) + (self.data[4] as u16)
    }

    #[inline]
    pub const fn c(&self) -> u16 {
        ((self.data[7] as u16) << 8) + (self.data[6] as u16)
    }

    #[inline]
    pub const fn d(&self) -> u16 {
        ((self.data[8] as u16) << 8) + (self.data[9] as u16)
    }

    #[inline]
    pub fn e(&self) -> &[u8] {
        &self.data[10..]
    }

    #[inline]
    pub fn e_u48(&self) -> u64 {
        self.e().iter().fold(0, |acc, v| (acc << 8) + (*v as u64))
    }
}

impl PartialEq for Guid {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        unsafe { *self.as_u128() == *other.as_u128() }
    }
}

impl From<uuid::Uuid> for Guid {
    #[inline]
    fn from(uuid: uuid::Uuid) -> Self {
        Self {
            data: uuid.into_raw(),
        }
    }
}

impl From<Guid> for uuid::Uuid {
    #[inline]
    fn from(guid: Guid) -> Self {
        uuid::Uuid::from_bytes(guid.data)
    }
}

pub enum ParseError {
    InvalidLength,
    InvalidDelimiter,
    InvalidDigit,
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            self.a(),
            self.b(),
            self.c(),
            self.d(),
            self.e_u48(),
        )
    }
}

#[macro_export]
macro_rules! guid {
    ($uuid:expr) => {{
        const GUID: $crate::Guid = $crate::Guid::parse_or_panic($uuid);
        GUID
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid() {
        let guid = guid!("00112233-4455-6677-8899-aabbccddeeff");
        assert_eq!(
            guid,
            Guid::from_raw([
                0x33, 0x22, 0x11, 0x00, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
                0xEE, 0xFF,
            ])
        );

        let guid = guid!("12345678-9abc-def0-fedc-ba9876543210");
        assert_eq!(
            guid,
            Guid::from_raw([
                0x78, 0x56, 0x34, 0x12, 0xbc, 0x9a, 0xf0, 0xde, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
                0x32, 0x10
            ])
        );
    }
}

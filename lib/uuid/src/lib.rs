//! Universally Unique Identifier (RFC 4122)
#![cfg_attr(not(test), no_std)]

extern crate alloc;
use core::{fmt, mem::transmute};
pub use uuid_identify::*;

/// Universally Unique Identifier (RFC 4122)
#[repr(transparent)]
#[derive(Copy, Clone, Eq)]
pub struct Uuid([u8; 16]);

impl Uuid {
    pub const NULL: Self = Self::null();

    #[inline]
    pub const fn from_parts(a: u32, b: u16, c: u16, d: u16, e: [u8; 6]) -> Self {
        let a = a.to_be_bytes();
        let b = b.to_be_bytes();
        let c = c.to_be_bytes();
        let d = d.to_be_bytes();
        Self([
            a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d[0], d[1], e[0], e[1], e[2], e[3],
            e[4], e[5],
        ])
    }

    #[inline]
    pub const fn from_raw(data: [u8; 16]) -> Self {
        Self(data)
    }

    #[inline]
    pub const fn a(&self) -> u32 {
        ((self.0[0] as u32) << 24)
            + ((self.0[1] as u32) << 16)
            + ((self.0[2] as u32) << 8)
            + (self.0[3] as u32)
    }

    #[inline]
    pub const fn b(&self) -> u16 {
        ((self.0[4] as u16) << 8) + (self.0[5] as u16)
    }

    #[inline]
    pub const fn c(&self) -> u16 {
        ((self.0[6] as u16) << 8) + (self.0[7] as u16)
    }

    #[inline]
    pub const fn d(&self) -> u16 {
        ((self.0[8] as u16) << 8) + (self.0[9] as u16)
    }

    #[inline]
    pub fn e(&self) -> &[u8] {
        &self.0[10..]
    }

    #[inline]
    pub fn e_u48(&self) -> u64 {
        self.e().iter().fold(0, |acc, v| (acc << 8) + (*v as u64))
    }

    #[inline]
    pub const fn null() -> Self {
        Self([0; 16])
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.eq(&Self::NULL)
    }

    #[inline]
    pub const fn into_raw(self) -> [u8; 16] {
        self.0
    }

    #[inline]
    pub const fn as_slice(&self) -> &[u8; 16] {
        &self.0
    }

    #[inline]
    pub const unsafe fn as_u128(&self) -> &u128 {
        unsafe { transmute(self) }
    }

    #[inline]
    pub fn version(&self) -> Option<UuidVersion> {
        unsafe { transmute(self.0[6] >> 4) }
    }
}

impl PartialEq for Uuid {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        unsafe { *self.as_u128() == *other.as_u128() }
    }
}

impl PartialOrd for Uuid {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.as_slice().partial_cmp(other.as_slice())
    }
}

impl Ord for Uuid {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl fmt::Debug for Uuid {
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

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UuidVersion {
    V1 = 1,
    V2,
    V3,
    V4,
    V5,
    V6,
    V7,
    V8,
    _V9,
    _V10,
    _V11,
    _V12,
    _V13,
    _V14,
    _V15,
}

pub unsafe trait Identify {
    const UUID: Uuid;
}

/// Globally Unique Identifier (MS-GUID)
#[repr(transparent)]
#[derive(Copy, Clone, Eq)]
pub struct Guid([u8; 16]);

impl Guid {
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
        Self(data)
    }

    #[inline]
    pub const unsafe fn as_u128(&self) -> &u128 {
        unsafe { transmute(self) }
    }

    #[inline]
    pub const fn a(&self) -> u32 {
        ((self.0[3] as u32) << 24)
            + ((self.0[2] as u32) << 16)
            + ((self.0[1] as u32) << 8)
            + (self.0[0] as u32)
    }

    #[inline]
    pub const fn b(&self) -> u16 {
        ((self.0[5] as u16) << 8) + (self.0[4] as u16)
    }

    #[inline]
    pub const fn c(&self) -> u16 {
        ((self.0[7] as u16) << 8) + (self.0[6] as u16)
    }

    #[inline]
    pub const fn d(&self) -> u16 {
        ((self.0[8] as u16) << 8) + (self.0[9] as u16)
    }

    #[inline]
    pub fn e(&self) -> &[u8] {
        &self.0[10..]
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

impl From<Uuid> for Guid {
    #[inline]
    fn from(uuid: Uuid) -> Self {
        Self(uuid.0)
    }
}

impl From<Guid> for Uuid {
    #[inline]
    fn from(guid: Guid) -> Self {
        Uuid(guid.0)
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
    use core::assert_eq;

    use super::*;

    #[test]
    fn uuid1() {
        let uuid1_raw = Uuid::from_raw([
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54,
            0x32, 0x10,
        ]);
        let uuid1 = Uuid::from_parts(
            0x1234_5678,
            0x9ABC,
            0xDEF0,
            0xFEDC,
            [0xBA, 0x98, 0x76, 0x54, 0x32, 0x10],
        );
        let uuid2_raw = Uuid::from_raw([
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
            0xEE, 0xFF,
        ]);
        let uuid2 = Uuid::from_parts(
            0x0011_2233,
            0x4455,
            0x6677,
            0x8899,
            [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
        );

        assert_eq!(uuid1, uuid1_raw);
        assert_eq!(uuid2, uuid2_raw);
        assert_ne!(uuid1, uuid2);

        assert_eq!(uuid1.a(), 0x1234_5678);
        assert_eq!(uuid1.b(), 0x9ABC);
        assert_eq!(uuid1.c(), 0xDEF0);
        assert_eq!(uuid1.d(), 0xFEDC);
        assert_eq!(uuid1.e_u48(), 0xBA98_7654_3210);

        assert_eq!(uuid2.a(), 0x0011_2233);
        assert_eq!(uuid2.b(), 0x4455);
        assert_eq!(uuid2.c(), 0x6677);
        assert_eq!(uuid2.d(), 0x8899);
        assert_eq!(uuid2.e_u48(), 0xAABB_CCDD_EEFF);
    }

    #[test]
    fn identify() {
        #[identify("12345678-9abc-def0-fedc-ba9876543210")]
        struct Foo;

        let uuid1_foo = Uuid::from_raw([
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54,
            0x32, 0x10,
        ]);

        assert_eq!(Foo::UUID, uuid1_foo);
    }

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

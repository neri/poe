use libhid::*;

pub struct HidManager;

impl HidManager {
    #[allow(unused)]
    #[inline]
    const fn new() -> Self {
        Self {}
    }

    /// Translates a HID usage and modifier state into a Unicode character, if possible.
    pub fn translate(usage: Usage, modifier: Modifier) -> Option<char> {
        let usage = usage.0;
        if usage >= 128 {
            // currently unsupported
            return None;
        }

        if modifier.has_alt() {
            return None;
        } else if modifier.has_ctrl() {
            let ascii = USAGE_TO_SHIFT[usage as usize];
            return (ascii != 0 && ascii >= 0x40 && ascii <= 0x7e).then(|| (ascii & 0x1f) as char);
        } else if modifier.has_shift() {
            let ascii = USAGE_TO_SHIFT[usage as usize];
            return (ascii != 0).then(|| ascii as char);
        } else {
            let ascii = USAGE_TO_ASCII[usage as usize];
            (ascii != 0).then(|| ascii as char)
        }
    }
}

#[rustfmt::skip]
const USAGE_TO_ASCII: [u8; 128] = [
    0x00, 0x00, 0x00, 0x00, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j', b'k', b'l',
    b'm', b'n', b'o', b'p', b'q', b'r', b's', b't', b'u', b'v', b'w', b'x', b'y', b'z', b'1', b'2',
    b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', 0x0d, 0x1b, 0x7f, b'\t', b' ', b'-', b'=', b'[',
    b']', b'\\', 0x00, b';', b'\'', b'`', b',', b'.', b'/', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
    0x00, 0x00, 0x00, 0x00, b'/', b'*', b'-', b'+', 0x0d, b'1', b'2', b'3', b'4', b'5', b'6', b'7',
    b'8', b'9', b'0', b'.', b'\\', 0x00, 0x00, b'=', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[rustfmt::skip]
const USAGE_TO_SHIFT: [u8; 128] = [
    0x00, 0x00, 0x00, 0x00, b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', b'I', b'J', b'K', b'L',
    b'M', b'N', b'O', b'P', b'Q', b'R', b'S', b'T', b'U', b'V', b'W', b'X', b'Y', b'Z', b'!', b'@',
    b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', 0x0d, 0x1b, 0x7f, b'\t', b' ', b'_', b'+', b'{',
    b'}', b'|', 0x00, b':', b'"', b'~', b'<', b'>', b'?', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
    0x00, 0x00, 0x00, 0x00, b'/', b'*', b'-', b'+', 0x0d, b'1', b'2', b'3', b'4', b'5', b'6', b'7',
    b'8', b'9', b'0', b'.', b'\\', 0x00, 0x00, b'=', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

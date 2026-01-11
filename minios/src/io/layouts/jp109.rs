//! JP 109-key keyboard layout

use super::*;

pub struct Jp109;

impl Jp109 {
    #[inline]
    pub const fn new() -> Self {
        Self
    }
}

impl Layout for Jp109 {
    fn translate(&self, key_stroke: KeyStroke) -> Option<char> {
        let usage = key_stroke.usage.0;
        if usage > 0x8f {
            // currently unsupported
            return None;
        }

        if key_stroke.modifier.has_alt() {
            return None;
        } else if key_stroke.modifier.has_ctrl() {
            let ascii = USAGE_TO_SHIFT[usage as usize];
            return (ascii != 0 && ascii >= 0x40 && ascii <= 0x7e).then(|| (ascii & 0x1f) as char);
        } else if key_stroke.modifier.has_shift() {
            let ascii = USAGE_TO_SHIFT[usage as usize];
            return (ascii != 0).then(|| ascii as char);
        } else {
            let ascii = USAGE_TO_ASCII[usage as usize];
            (ascii != 0).then(|| ascii as char)
        }
    }

    fn infer_key_stroke_from_char(&self, c: char) -> Option<KeyStroke> {
        // TODO: refactor
        let ascii = c as u32;
        if ascii == 0 || ascii > 0x7f {
            return None;
        }

        // Search in unshifted table first
        for (usage, &ch) in USAGE_TO_ASCII.iter().enumerate() {
            if ch as u32 == ascii {
                return Some(KeyStroke {
                    usage: Usage(usage as u8),
                    modifier: Modifier::empty(),
                });
            }
        }

        // Then search in shifted table
        for (usage, &ch) in USAGE_TO_SHIFT.iter().enumerate() {
            if ch as u32 == ascii {
                return Some(KeyStroke {
                    usage: Usage(usage as u8),
                    modifier: Modifier::LEFT_SHIFT,
                });
            }
        }

        None
    }
}

#[rustfmt::skip]
const USAGE_TO_ASCII: [u8; 144] = [
    /* 0- */ 0x00, 0x00, 0x00, 0x00, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j', b'k', b'l',
    /* 1- */ b'm', b'n', b'o', b'p', b'q', b'r', b's', b't', b'u', b'v', b'w', b'x', b'y', b'z', b'1', b'2',
    /* 2- */ b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', 0x0d, 0x1b, 0x08, b'\t', b' ', b'-', b'^', b'@',
    /* 3- */ b'[', b']', b']', b';', b':', b'`', b',', b'.', b'/', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    /* 4- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
    /* 5- */ 0x00, 0x00, 0x00, 0x00, b'/', b'*', b'-', b'+', 0x0d, b'1', b'2', b'3', b'4', b'5', b'6', b'7',
    /* 6- */ b'8', b'9', b'0', b'.', b'\\', 0x00, 0x00, b'=', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    /* 7- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    /* 8- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'\\', 0x00, b'\\', 0x00, 0x00, b',', 0x00, 0x00, 0x00,
];

#[rustfmt::skip]
const USAGE_TO_SHIFT: [u8; 144] = [
    /* 0- */ 0x00, 0x00, 0x00, 0x00, b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', b'I', b'J', b'K', b'L',
    /* 1- */ b'M', b'N', b'O', b'P', b'Q', b'R', b'S', b'T', b'U', b'V', b'W', b'X', b'Y', b'Z', b'!', b'"',
    /* 2- */ b'#', b'$', b'%', b'&', b'\'', b'(', b')', 0x00, 0x0d, 0x1b, 0x7f, b'\t', b' ', b'=', b'~', b'`',
    /* 3- */ b'{', b'}', b'}', b'+', b'*', b'~', b'<', b'>', b'?', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    /* 4- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
    /* 5- */ 0x00, 0x00, 0x00, 0x00, b'/', b'*', b'-', b'+', 0x0d, b'1', b'2', b'3', b'4', b'5', b'6', b'7',
    /* 6- */ b'8', b'9', b'0', b'.', b'\\', 0x00, 0x00, b'=', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    /* 7- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    /* 8- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'_', 0x00, b'|', 0x00, 0x00, b',', 0x00, 0x00, 0x00,
];

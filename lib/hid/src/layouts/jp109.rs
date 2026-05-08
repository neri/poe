//! JP 109-key keyboard layout

use super::*;

pub struct Jp109;

impl Jp109 {
    #[inline]
    pub const fn new() -> Self {
        Self
    }
}

impl KeyboardLayout for Jp109 {
    fn translate(&self, key_stroke: KeyStroke) -> Option<char> {
        let usage = key_stroke.usage.0;
        if usage as usize >= USAGE_TO_ASCII.len() {
            // currently unsupported
            return None;
        }

        if key_stroke.modifier.has_menu() || key_stroke.modifier.has_alt() {
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

    fn estimate_key_stroke_from_char(&self, c: char) -> Option<KeyStroke> {
        let (usage, modifier) = *CHAR_TO_KEYSTROKE.get(c as usize)?;
        (usage != Usage::NONE).then(|| KeyStroke { usage, modifier })
    }
}

#[rustfmt::skip]
const USAGE_TO_ASCII: [u8; 144] = [
    /* 0- */ 0x00, 0x00, 0x00, 0x00, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j', b'k', b'l',
    /* 1- */ b'm', b'n', b'o', b'p', b'q', b'r', b's', b't', b'u', b'v', b'w', b'x', b'y', b'z', b'1', b'2',
    /* 2- */ b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', 0x0d, 0x1b, 0x08, b'\t', b' ', b'-', b'^', b'@',
    /* 3- */ b'[', b']', b']', b';', b':', b'`', b',', b'.', b'/', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    /* 4- */ 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7f, 0x00, 0x00, 0x00, 
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

/// Infer mapping from character to KeyStroke
const CHAR_TO_KEYSTROKE: [(Usage, Modifier); 128] = [
    (Usage::NONE, Modifier::empty()),          // 0x00
    (Usage::KEY_A, Modifier::LEFT_CTRL),       // 0x01
    (Usage::KEY_B, Modifier::LEFT_CTRL),       // 0x02
    (Usage::KEY_C, Modifier::LEFT_CTRL),       // 0x03
    (Usage::KEY_D, Modifier::LEFT_CTRL),       // 0x04
    (Usage::KEY_E, Modifier::LEFT_CTRL),       // 0x05
    (Usage::KEY_F, Modifier::LEFT_CTRL),       // 0x06
    (Usage::KEY_G, Modifier::LEFT_CTRL),       // 0x07
    (Usage::KEY_BACKSPACE, Modifier::empty()), // 0x08 (BACKSPACE)
    (Usage::KEY_TAB, Modifier::empty()),       // 0x09 (HTAB)
    (Usage::KEY_J, Modifier::LEFT_CTRL),       // 0x0A
    (Usage::KEY_K, Modifier::LEFT_CTRL),       // 0x0B
    (Usage::KEY_L, Modifier::LEFT_CTRL),       // 0x0C
    (Usage::KEY_ENTER, Modifier::empty()),     // 0x0D (ENTER)
    (Usage::KEY_N, Modifier::LEFT_CTRL),       // 0x0E
    (Usage::KEY_O, Modifier::LEFT_CTRL),       // 0x0F
    (Usage::KEY_P, Modifier::LEFT_CTRL),       // 0x10
    (Usage::KEY_Q, Modifier::LEFT_CTRL),       // 0x11
    (Usage::KEY_R, Modifier::LEFT_CTRL),       // 0x12
    (Usage::KEY_S, Modifier::LEFT_CTRL),       // 0x13
    (Usage::KEY_T, Modifier::LEFT_CTRL),       // 0x14
    (Usage::KEY_U, Modifier::LEFT_CTRL),       // 0x15
    (Usage::KEY_V, Modifier::LEFT_CTRL),       // 0x16
    (Usage::KEY_W, Modifier::LEFT_CTRL),       // 0x17
    (Usage::KEY_X, Modifier::LEFT_CTRL),       // 0x18
    (Usage::KEY_Y, Modifier::LEFT_CTRL),       // 0x19
    (Usage::KEY_Z, Modifier::LEFT_CTRL),       // 0x1A
    (Usage::KEY_ESCAPE, Modifier::empty()),    // 0x1B (ESCAPE)
    (Usage(0x89), Modifier::LEFT_CTRL),        // 0x1c
    (Usage(0x31), Modifier::LEFT_CTRL),        // 0x1d
    (Usage(0x2e), Modifier::LEFT_CTRL),        // 0x1e
    (Usage(0x87), Modifier::LEFT_CTRL),        // 0x1f
    (Usage::KEY_SPACE, Modifier::empty()),     // 0x20
    (Usage::KEY_1, Modifier::LEFT_SHIFT),      // 0x21
    (Usage::KEY_2, Modifier::LEFT_SHIFT),      // 0x22
    (Usage::KEY_3, Modifier::LEFT_SHIFT),      // 0x23
    (Usage::KEY_4, Modifier::LEFT_SHIFT),      // 0x24
    (Usage::KEY_5, Modifier::LEFT_SHIFT),      // 0x25
    (Usage::KEY_6, Modifier::LEFT_SHIFT),      // 0x26
    (Usage::KEY_7, Modifier::LEFT_SHIFT),      // 0x27
    (Usage::KEY_8, Modifier::LEFT_SHIFT),      // 0x28
    (Usage::KEY_9, Modifier::LEFT_SHIFT),      // 0x29
    (Usage(0x34), Modifier::LEFT_SHIFT),       // 0x2a
    (Usage(0x33), Modifier::LEFT_SHIFT),       // 0x2b
    (Usage(0x36), Modifier::empty()),          // 0x2c
    (Usage(0x2d), Modifier::empty()),          // 0x2d
    (Usage(0x37), Modifier::empty()),          // 0x2e
    (Usage(0x38), Modifier::empty()),          // 0x2f
    (Usage::KEY_0, Modifier::empty()),         // 0x30
    (Usage::KEY_1, Modifier::empty()),         // 0x31
    (Usage::KEY_2, Modifier::empty()),         // 0x32
    (Usage::KEY_3, Modifier::empty()),         // 0x33
    (Usage::KEY_4, Modifier::empty()),         // 0x34
    (Usage::KEY_5, Modifier::empty()),         // 0x35
    (Usage::KEY_6, Modifier::empty()),         // 0x36
    (Usage::KEY_7, Modifier::empty()),         // 0x37
    (Usage::KEY_8, Modifier::empty()),         // 0x38
    (Usage::KEY_9, Modifier::empty()),         // 0x39
    (Usage(0x34), Modifier::empty()),          // 0x3a
    (Usage(0x33), Modifier::empty()),          // 0x3b
    (Usage(0x36), Modifier::LEFT_SHIFT),       // 0x3c
    (Usage(0x2d), Modifier::LEFT_SHIFT),       // 0x3d
    (Usage(0x37), Modifier::LEFT_SHIFT),       // 0x3e
    (Usage(0x38), Modifier::LEFT_SHIFT),       // 0x3f
    (Usage(0x2f), Modifier::empty()),          // 0x40
    (Usage::KEY_A, Modifier::LEFT_SHIFT),      // 0x41
    (Usage::KEY_B, Modifier::LEFT_SHIFT),      // 0x42
    (Usage::KEY_C, Modifier::LEFT_SHIFT),      // 0x43
    (Usage::KEY_D, Modifier::LEFT_SHIFT),      // 0x44
    (Usage::KEY_E, Modifier::LEFT_SHIFT),      // 0x45
    (Usage::KEY_F, Modifier::LEFT_SHIFT),      // 0x46
    (Usage::KEY_G, Modifier::LEFT_SHIFT),      // 0x47
    (Usage::KEY_H, Modifier::LEFT_SHIFT),      // 0x48
    (Usage::KEY_I, Modifier::LEFT_SHIFT),      // 0x49
    (Usage::KEY_J, Modifier::LEFT_SHIFT),      // 0x4a
    (Usage::KEY_K, Modifier::LEFT_SHIFT),      // 0x4b
    (Usage::KEY_L, Modifier::LEFT_SHIFT),      // 0x4c
    (Usage::KEY_M, Modifier::LEFT_SHIFT),      // 0x4d
    (Usage::KEY_N, Modifier::LEFT_SHIFT),      // 0x4e
    (Usage::KEY_O, Modifier::LEFT_SHIFT),      // 0x4f
    (Usage::KEY_P, Modifier::LEFT_SHIFT),      // 0x50
    (Usage::KEY_Q, Modifier::LEFT_SHIFT),      // 0x51
    (Usage::KEY_R, Modifier::LEFT_SHIFT),      // 0x52
    (Usage::KEY_S, Modifier::LEFT_SHIFT),      // 0x53
    (Usage::KEY_T, Modifier::LEFT_SHIFT),      // 0x54
    (Usage::KEY_U, Modifier::LEFT_SHIFT),      // 0x55
    (Usage::KEY_V, Modifier::LEFT_SHIFT),      // 0x56
    (Usage::KEY_W, Modifier::LEFT_SHIFT),      // 0x57
    (Usage::KEY_X, Modifier::LEFT_SHIFT),      // 0x58
    (Usage::KEY_Y, Modifier::LEFT_SHIFT),      // 0x59
    (Usage::KEY_Z, Modifier::LEFT_SHIFT),      // 0x5a
    (Usage(0x30), Modifier::empty()),          // 0x5b
    (Usage(0x89), Modifier::empty()),          // 0x5c
    (Usage(0x31), Modifier::empty()),          // 0x5d
    (Usage(0x2e), Modifier::empty()),          // 0x5e
    (Usage(0x87), Modifier::LEFT_SHIFT),       // 0x5f
    (Usage(0x2f), Modifier::LEFT_SHIFT),       // 0x60
    (Usage::KEY_A, Modifier::empty()),         // 0x61
    (Usage::KEY_B, Modifier::empty()),         // 0x62
    (Usage::KEY_C, Modifier::empty()),         // 0x63
    (Usage::KEY_D, Modifier::empty()),         // 0x64
    (Usage::KEY_E, Modifier::empty()),         // 0x65
    (Usage::KEY_F, Modifier::empty()),         // 0x66
    (Usage::KEY_G, Modifier::empty()),         // 0x67
    (Usage::KEY_H, Modifier::empty()),         // 0x68
    (Usage::KEY_I, Modifier::empty()),         // 0x69
    (Usage::KEY_J, Modifier::empty()),         // 0x6a
    (Usage::KEY_K, Modifier::empty()),         // 0x6b
    (Usage::KEY_L, Modifier::empty()),         // 0x6c
    (Usage::KEY_M, Modifier::empty()),         // 0x6d
    (Usage::KEY_N, Modifier::empty()),         // 0x6e
    (Usage::KEY_O, Modifier::empty()),         // 0x6f
    (Usage::KEY_P, Modifier::empty()),         // 0x70
    (Usage::KEY_Q, Modifier::empty()),         // 0x71
    (Usage::KEY_R, Modifier::empty()),         // 0x72
    (Usage::KEY_S, Modifier::empty()),         // 0x73
    (Usage::KEY_T, Modifier::empty()),         // 0x74
    (Usage::KEY_U, Modifier::empty()),         // 0x75
    (Usage::KEY_V, Modifier::empty()),         // 0x76
    (Usage::KEY_W, Modifier::empty()),         // 0x77
    (Usage::KEY_X, Modifier::empty()),         // 0x78
    (Usage::KEY_Y, Modifier::empty()),         // 0x79
    (Usage::KEY_Z, Modifier::empty()),         // 0x7a
    (Usage(0x30), Modifier::LEFT_SHIFT),       // 0x7b
    (Usage(0x89), Modifier::LEFT_SHIFT),       // 0x7c
    (Usage(0x31), Modifier::LEFT_SHIFT),       // 0x7d
    (Usage(0x2e), Modifier::LEFT_SHIFT),       // 0x7e
    (Usage::KEY_DELETE, Modifier::empty()),    // 0x7f
];

#[test]
fn test_jp109() {
    super::tests::layout_test(&Jp109);
}

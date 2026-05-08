use super::*;

pub mod jp109;
pub mod us101;

/// Represents a keystroke, consisting of a HID usage and modifier state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    pub usage: Usage,
    pub modifier: Modifier,
}

impl KeyStroke {
    /// Creates a new KeyStroke with the given usage and modifier.
    #[inline]
    pub const fn new(usage: Usage, modifier: Modifier) -> Self {
        Self { usage, modifier }
    }

    /// Creates a new KeyStroke with the given usage and empty modifier.
    #[inline]
    pub const fn from_usage(usage: Usage) -> Self {
        Self::new(usage, Modifier::empty())
    }
}

pub trait KeyboardLayout {
    /// Translates a HID usage and modifier state into a Unicode character, if possible.
    fn translate(&self, key_stroke: KeyStroke) -> Option<char>;

    /// Estimates a KeyStroke from a Unicode character, if possible.
    fn estimate_key_stroke_from_char(&self, c: char) -> Option<KeyStroke>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    pub fn layout_test(layout: &dyn KeyboardLayout) {
        assert_eq!(layout.estimate_key_stroke_from_char('\0'), None);

        // Keystroke estimation from ASCII codes isn't perfect.
        // However, we guarantee that the estimated keystrokes can be reversed using the current layout.
        for ch in 1..128 {
            let ch = ch as u8 as char;
            println!("Testing character {:?}", ch);

            let key_stroke = layout.estimate_key_stroke_from_char(ch).unwrap();

            let translated_char = layout.translate(key_stroke).unwrap();

            assert_eq!(
                ch, translated_char,
                "Character translation mismatch: expected {:?}, got {:?}",
                ch, translated_char
            );
        }

        // for ch in 128..256 {
        //     assert_eq!(layout.estimate_key_stroke_from_char(ch as u8 as char), None);
        // }
    }
}

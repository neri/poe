use super::*;

pub mod jp109;
pub mod us101;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    pub usage: Usage,
    pub modifier: Modifier,
}

impl KeyStroke {
    #[inline]
    pub fn new(usage: Usage, modifier: Modifier) -> Self {
        Self { usage, modifier }
    }
}

pub trait KeyboardLayout {
    /// Translates a HID usage and modifier state into a Unicode character, if possible.
    fn translate(&self, key_stroke: KeyStroke) -> Option<char>;

    /// Estimates a KeyStroke from a Unicode character, if possible.
    fn estimate_key_stroke_from_char(&self, c: char) -> Option<KeyStroke>;
}

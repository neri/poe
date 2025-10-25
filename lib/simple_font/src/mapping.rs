pub trait GlyphMapping {
    fn map_char(&self, ch: char) -> Option<usize>;
}

pub static ASCII: AsciiMapping = AsciiMapping;

pub struct AsciiMapping;

impl GlyphMapping for AsciiMapping {
    #[inline]
    fn map_char(&self, ch: char) -> Option<usize> {
        let code = ch as usize;
        match code {
            32..=126 => Some(code - 32),
            _ => None,
        }
    }
}

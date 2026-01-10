//! Human Interface Device (HID) manager

#[path = "layouts/mod.rs"]
pub mod layouts;

use crate::*;
use core::cell::UnsafeCell;
use libhid::*;

static mut HID_MGR: UnsafeCell<HidManager> = UnsafeCell::new(HidManager::new());

// Default to US 101-key layout
static DEFAULT_LAYOUT: layouts::us101::Us101 = layouts::us101::Us101;

pub struct HidManager {
    layout: Option<Box<dyn Layout>>,
}

impl HidManager {
    #[allow(unused)]
    #[inline]
    const fn new() -> Self {
        Self { layout: None }
    }

    #[inline]
    unsafe fn shared_mut<'a>() -> &'a mut Self {
        unsafe { (&mut *(&raw mut HID_MGR)).get_mut() }
    }

    #[inline]
    pub fn set_japanese_layout() {
        let shared = unsafe { Self::shared_mut() };
        let layout = layouts::jp109::Jp109;
        shared.layout = Some(Box::new(layout));
    }

    pub fn set_layout(layout: Box<dyn Layout>) {
        let shared = unsafe { Self::shared_mut() };
        shared.layout = Some(layout);
    }

    pub fn current_layout<'a>() -> &'a dyn Layout {
        let shared = unsafe { Self::shared_mut() };
        match shared.layout.as_ref() {
            Some(v) => v.as_ref(),
            None => &DEFAULT_LAYOUT,
        }
    }

    /// Translates a HID usage and modifier state into a Unicode character, if possible.
    pub fn translate(key_stroke: KeyStroke) -> Option<char> {
        Self::current_layout().translate(key_stroke)
    }

    /// Infers a KeyStroke from a Unicode character, if possible.
    pub fn infer_key_stroke_from_char(c: char) -> Option<KeyStroke> {
        Self::current_layout().infer_key_stroke_from_char(c)
    }
}

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

pub trait Layout {
    /// Translates a HID usage and modifier state into a Unicode character, if possible.
    fn translate(&self, key_stroke: KeyStroke) -> Option<char>;

    /// Infers a KeyStroke from a Unicode character, if possible.
    fn infer_key_stroke_from_char(&self, c: char) -> Option<KeyStroke>;
}

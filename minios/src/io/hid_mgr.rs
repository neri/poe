//! Human Interface Device (HID) manager

use core::cell::UnsafeCell;

pub use layouts::KeyStroke;
use libhid::layouts::*;
use libhid::*;

use crate::*;

static mut HID_MGR: UnsafeCell<HidManager> = UnsafeCell::new(HidManager::new());

// Default to US 101-key layout
static DEFAULT_LAYOUT: layouts::us101::Us101 = layouts::us101::Us101;

pub struct HidManager {
    layout: Option<Box<dyn KeyboardLayout>>,
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

    /// Sets the current keyboard layout to the japanese 109-key layout, because NEC PC-98 series and Fujitsu FM TOWNS series use this layout.
    #[inline]
    pub fn set_japanese_layout() {
        let shared = unsafe { Self::shared_mut() };
        let layout = layouts::jp109::Jp109;
        shared.layout = Some(Box::new(layout));
    }

    /// Sets the current keyboard layout to the specified layout.
    #[inline]
    pub fn set_layout(layout: Box<dyn KeyboardLayout>) {
        let shared = unsafe { Self::shared_mut() };
        shared.layout = Some(layout);
    }

    /// Returns the current keyboard layout.
    pub fn current_layout<'a>() -> &'a dyn KeyboardLayout {
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

    /// Estimate a KeyStroke from a Unicode character, if possible.
    pub fn estimate_key_stroke_from_char(c: char) -> Option<KeyStroke> {
        Self::current_layout().estimate_key_stroke_from_char(c)
    }
}

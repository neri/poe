// use super::*;
use crate::{vt100::VT100, *};
use core::{cell::UnsafeCell, fmt};

pub struct SbiConsole;

static mut RAW: UnsafeCell<SbiConsole> = UnsafeCell::new(SbiConsole {});

static mut SHARED: UnsafeCell<VT100> =
    UnsafeCell::new(VT100::new(unsafe { &mut *(&raw mut RAW) }.get_mut()));

impl SbiConsole {
    #[inline]
    pub unsafe fn init() {
        //
    }

    #[inline]
    pub fn shared_in() -> &'static mut SbiConsole {
        unsafe { (&mut *(&raw mut RAW)).get_mut() }
    }

    #[inline]
    pub fn shared_out() -> &'static mut VT100<'static> {
        unsafe { (&mut *(&raw mut SHARED)).get_mut() }
    }

    #[inline]
    fn write_byte(&self, ch: u8) {
        sbi::legacy::putchar(ch);
    }

    #[inline]
    fn read_byte(&self) -> u8 {
        loop {
            match sbi::legacy::getchar() {
                Some(v) => return v,
                None => {}
            }
        }
    }
}

impl fmt::Write for SbiConsole {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for ch in s.bytes() {
            self.write_byte(ch);
        }
        Ok(())
    }
}

impl SimpleTextOutput for SbiConsole {
    fn reset(&mut self) {
        //
    }

    fn set_attribute(&mut self, _attribute: u8) {
        //
    }

    fn clear_screen(&mut self) {
        //
    }

    fn set_cursor_position(&mut self, _col: u32, _row: u32) {
        //
    }

    fn enable_cursor(&mut self, _visible: bool) -> bool {
        false
    }

    fn current_mode(&self) -> SimpleTextOutputMode {
        SimpleTextOutputMode {
            columns: 80,
            rows: 24,
            cursor_column: 0,
            cursor_row: 0,
            attribute: 0,
            cursor_visible: 0,
        }
    }
}

impl SimpleTextInput for SbiConsole {
    fn reset(&mut self) {
        //
    }

    fn read_key_stroke(&mut self) -> Option<NonZeroInputKey> {
        // if !self.is_input_ready() {
        //     return None;
        // }
        let ch = self.read_byte();
        NonZeroInputKey::new(0xffff, ch as u16)
    }
}

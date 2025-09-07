// use super::*;
use crate::{vt100::VT100, *};
use core::cell::UnsafeCell;

pub struct SbiConsole;

static mut RAW: UnsafeCell<SbiConsole> = UnsafeCell::new(SbiConsole {});

static mut SHARED: UnsafeCell<VT100> = UnsafeCell::new(VT100::new(SbiConsole::shared_raw()));

impl SbiConsole {
    #[inline]
    pub unsafe fn init() {
        Self::shared_raw().reset();
    }

    #[inline]
    const fn shared_raw() -> &'static mut SbiConsole {
        unsafe { (&mut *(&raw mut RAW)).get_mut() }
    }

    #[inline]
    pub fn shared() -> &'static mut VT100<'static> {
        unsafe { (&mut *(&raw mut SHARED)).get_mut() }
    }
}

impl SerialIo for SbiConsole {
    #[inline]
    fn reset(&mut self) {
        //
    }

    #[inline]
    fn write_byte(&mut self, ch: u8) {
        sbi::legacy::putchar(ch);
    }

    #[inline]
    fn read_byte(&mut self) -> Option<u8> {
        sbi::legacy::getchar()
    }
}

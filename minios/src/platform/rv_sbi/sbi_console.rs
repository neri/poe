//! Console implementation using SBI

use crate::{vt100::VT100, *};
use core::cell::UnsafeCell;

pub struct SbiConsole {
    last_input: Option<u8>,
}

static mut RAW: UnsafeCell<SbiConsole> = UnsafeCell::new(SbiConsole::new());

static mut SHARED: UnsafeCell<VT100> = UnsafeCell::new(VT100::new(SbiConsole::shared_raw()));

impl SbiConsole {
    #[inline]
    const fn new() -> Self {
        Self { last_input: None }
    }

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

    #[inline]
    fn refill(&mut self) {
        if self.last_input.is_none() {
            self.last_input = sbi::legacy::getchar();
        }
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
        self.is_ready_to_read()
            .then(|| self.last_input.take())
            .flatten()
    }

    #[inline]
    fn is_ready_to_read(&mut self) -> bool {
        self.refill();
        self.last_input.is_some()
    }
}

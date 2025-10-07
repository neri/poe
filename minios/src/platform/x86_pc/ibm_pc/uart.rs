//! Console implementation using UART

use crate::{vt100::VT100, *};
use core::cell::UnsafeCell;
use x86::isolated_io::{IoPortRB, IoPortWB};

pub struct Uart16550 {
    base_port: u16,
}
static mut RAW: UnsafeCell<Uart16550> = UnsafeCell::new(Uart16550 { base_port: 0x3f8 });

static mut SHARED: UnsafeCell<VT100> = UnsafeCell::new(VT100::new(Uart16550::shared_raw()));

impl Uart16550 {
    #[inline]
    pub unsafe fn init(base_port: u16) {
        unsafe {
            let uart = Self::shared_raw();
            uart.base_port = base_port;

            // Disable all interrupts
            IoPortWB(uart.base_port + 1).write(0x00);
            // Enable DLAB (set baud rate divisor)
            IoPortWB(uart.base_port + 3).write(0x80);
            // Set divisor to 3 (lo byte) 38400 baud
            IoPortWB(uart.base_port + 0).write(0x03);
            //                  (hi byte)
            IoPortWB(uart.base_port + 1).write(0x00);
            // 8 bits, no parity, one stop bit
            IoPortWB(uart.base_port + 3).write(0x03);
            // Enable FIFO, clear them, with 14-byte threshold
            IoPortWB(uart.base_port + 2).write(0xC7);
            // IRQs enabled, RTS/DSR set
            IoPortWB(uart.base_port + 4).write(0x0B);
        }
    }

    #[inline]
    const fn shared_raw() -> &'static mut Uart16550 {
        unsafe { (&mut *(&raw mut RAW)).get_mut() }
    }

    #[inline]
    pub fn shared() -> &'static mut VT100<'static> {
        unsafe { (&mut *(&raw mut SHARED)).get_mut() }
    }
}

impl SerialIo for Uart16550 {
    #[inline]
    fn reset(&mut self) {
        //
    }

    #[inline]
    fn write_byte(&mut self, byte: u8) {
        unsafe {
            while (IoPortRB(self.base_port + 5).read() & 0x20) == 0 {}
            IoPortWB(self.base_port).write(byte);
        }
    }

    #[inline]
    fn read_byte(&mut self) -> Option<u8> {
        unsafe {
            if (IoPortRB(self.base_port + 5).read() & 0x01) != 0 {
                Some(IoPortRB(self.base_port).read())
            } else {
                None
            }
        }
    }
}

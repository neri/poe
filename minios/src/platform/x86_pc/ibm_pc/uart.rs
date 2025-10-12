//! Console implementation using UART

use crate::{vt100::VT100, *};
use core::cell::UnsafeCell;
use x86::isolated_io::{IoPortRB, IoPortRWB, IoPortWB};

static mut RAW: UnsafeCell<Uart16550> = UnsafeCell::new(Uart16550::new());
static mut SHARED: UnsafeCell<VT100> = UnsafeCell::new(VT100::new(Uart16550::shared_raw()));

pub struct Uart16550 {
    base_port: u16,
    uart_type: UartType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UartType {
    _8250,
    _16450,
    _16550,
    _16550A,
    _16750,
}

impl Uart16550 {
    #[inline]
    const fn new() -> Self {
        Self {
            base_port: 0x3f8,
            uart_type: UartType::_8250,
        }
    }

    #[inline]
    pub unsafe fn init(base_port: u16) {
        unsafe {
            let uart = Self::shared_raw();
            uart.base_port = base_port;

            // Disable all interrupts
            IoPortWB(uart.base_port + 1).write(0x00);

            // Identify UART type
            IoPortWB(uart.base_port + 2).write(0xe7);
            let test = IoPortRB(uart.base_port + 2).read();
            uart.uart_type = match test {
                0xe0.. => UartType::_16750,
                0xc0..=0xdf => UartType::_16550A,
                0x80..=0xbf => UartType::_16550,
                _ => {
                    let scr = IoPortRWB(uart.base_port + 7);
                    scr.write(0x2a);
                    if scr.read() == 0x2a {
                        UartType::_16450
                    } else {
                        UartType::_8250
                    }
                }
            };

            // Enable DLAB (set baud rate divisor)
            IoPortWB(uart.base_port + 3).write(0x80);
            let baud = 115200 / 115200;
            // Set divisor (lo byte)
            IoPortWB(uart.base_port + 0).write((baud & 0xff) as u8);
            //             (hi byte)
            IoPortWB(uart.base_port + 1).write(((baud >> 8) & 0xff) as u8);

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

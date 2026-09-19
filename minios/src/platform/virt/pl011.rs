//! Arm PrimeCell UART (PL011)

use core::cell::UnsafeCell;

use crate::vt100::VT100;
use crate::*;

static mut PL011: Pl011 = Pl011::new(Pl011::DEFAULT_BASE);

static mut SHARED: UnsafeCell<VT100> = UnsafeCell::new(VT100::new(Pl011::shared_raw()));

/// Arm PrimeCell UART (PL011)
pub struct Pl011 {
    base: usize,
}

#[allow(dead_code)]
impl Pl011 {
    /// Base address of the PL011 on the QEMU virt machine
    pub const DEFAULT_BASE: usize = 0x0900_0000;

    const DR: usize = 0x00;
    const FR: usize = 0x18;
    const LCRH: usize = 0x2c;
    const CR: usize = 0x30;
    const IMSC: usize = 0x38;
    const ICR: usize = 0x44;

    const FR_BUSY: u32 = 0x08;
    const FR_RXFE: u32 = 0x10;
    const FR_TXFF: u32 = 0x20;

    #[inline]
    const fn new(base: usize) -> Self {
        Self { base }
    }

    #[inline]
    pub const fn shared_raw<'a>() -> &'a mut Self {
        unsafe { &mut *(&raw mut PL011) }
    }

    #[inline]
    pub const fn shared() -> &'static mut VT100<'static> {
        unsafe { &mut *(&raw mut SHARED) }.get_mut()
    }

    /// Initialize the UART at `base`.
    ///
    /// The baud rate is left as configured by the firmware,
    /// since the reference clock is not known here.
    pub unsafe fn init(base: usize) -> &'static mut Self {
        let shared = Self::shared_raw();
        shared.base = base;
        unsafe {
            // Wait for the firmware's output to drain, then disable UART.
            while (shared.read_reg(Self::FR) & Self::FR_BUSY) != 0 {
                Hal::cpu().no_op();
            }
            shared.write_reg(Self::CR, 0);

            // Clear pending interrupts.
            shared.write_reg(Self::ICR, 0x7ff);

            // Enable FIFO & 8 bit data transmission (1 stop bit, no parity).
            shared.write_reg(Self::LCRH, 0x0070);

            // Mask all interrupts.
            shared.write_reg(Self::IMSC, 0);

            // Enable UART, receive & transfer part of UART.
            shared.write_reg(Self::CR, 0x301);
        }
        shared
    }

    #[inline]
    unsafe fn read_reg(&self, offset: usize) -> u32 {
        unsafe { ((self.base + offset) as *const u32).read_volatile() }
    }

    #[inline]
    unsafe fn write_reg(&mut self, offset: usize, value: u32) {
        unsafe { ((self.base + offset) as *mut u32).write_volatile(value) }
    }
}

impl SerialIo for Pl011 {
    #[inline]
    fn reset(&mut self) {
        //
    }

    #[inline]
    fn write_byte(&mut self, byte: u8) {
        unsafe {
            while (self.read_reg(Self::FR) & Self::FR_TXFF) != 0 {
                Hal::cpu().no_op();
            }
            self.write_reg(Self::DR, byte as u32);
        }
    }

    #[inline]
    fn read_byte(&mut self) -> Option<u8> {
        if self.is_ready_to_read() {
            Some(unsafe { self.read_reg(Self::DR) as u8 })
        } else {
            None
        }
    }

    #[inline]
    fn is_ready_to_read(&mut self) -> bool {
        unsafe { (self.read_reg(Self::FR) & Self::FR_RXFE) == 0 }
    }
}

//! Generic Uart driver
use crate::{vt100::VT100, *};
use core::cell::UnsafeCell;

static mut RAW: UnsafeCell<Uart16550> = UnsafeCell::new(Uart16550::new());

static mut SHARED: UnsafeCell<VT100> = UnsafeCell::new(VT100::new(Uart16550::shared_raw()));

pub struct Uart16550 {
    base_address: usize,
}

#[allow(unused)]
#[derive(Debug, Clone, Copy)]
enum Register {
    DATA = 0,
    /// Interrupt Enable Register
    IER = 1,
    /// Interrupt Identification Register
    IIR = 2,
    /// Line Control Register
    LCR = 3,
    /// Modem Control Register
    MCR = 4,
    /// Line Status Register
    LSR = 5,
    /// Modem Status Register
    MSR = 6,
    /// Scratch Register
    SCRATCH = 7,
}

#[allow(unused)]
impl Register {
    /// Divisor Latch Low Byte (same address as DATA, but when DLAB=1)
    pub const DLL: Self = Self::DATA;

    /// Divisor Latch High Byte (same address as IER, but when DLAB=1)
    pub const DLM: Self = Self::IER;

    /// FIFO Control Register (same address as IIR, but write-only)
    pub const FCR: Self = Self::IIR;
}

impl Uart16550 {
    #[inline]
    const fn new() -> Self {
        Self { base_address: 0 }
    }

    #[inline]
    pub unsafe fn init(base_address: usize) {
        let uart = Self::shared_raw();
        uart.base_address = base_address;

        uart._write(Register::IER, 0x00);

        // uart._write(Register::LCR, 0x80);
        // uart._write(Register::DLL, 0x00);
        // uart._write(Register::DLM, 0x00);

        uart._write(Register::LCR, 0x03);

        uart._write(Register::FCR, 0x01);
        uart._write(Register::MCR, 0x00);
    }

    #[inline]
    pub const fn shared_raw() -> &'static mut Uart16550 {
        unsafe { (&mut *(&raw mut RAW)).get_mut() }
    }

    #[inline]
    pub fn shared() -> &'static mut VT100<'static> {
        unsafe { (&mut *(&raw mut SHARED)).get_mut() }
    }

    #[inline]
    fn _read(&self, reg: Register) -> u8 {
        unsafe {
            let p = (self.base_address as *const u8).add(reg as usize);
            let r = core::ptr::read_volatile(p);
            r
        }
    }

    #[inline]
    fn _write(&self, reg: Register, value: u8) {
        unsafe {
            let p = (self.base_address as *mut u8).add(reg as usize);
            core::ptr::write_volatile(p, value);
        }
    }

    #[inline]
    fn is_ready_to_write(&mut self) -> bool {
        self._read(Register::LSR) & 0x20 != 0
    }
}

impl SerialIo for Uart16550 {
    #[inline]
    fn reset(&mut self) {
        //
    }

    #[inline]
    fn write_byte(&mut self, byte: u8) {
        while !self.is_ready_to_write() {
            Hal::cpu().no_op();
        }
        self._write(Register::DATA, byte);
    }

    #[inline]
    fn is_ready_to_read(&mut self) -> bool {
        self._read(Register::LSR) & 0x01 != 0
    }

    #[inline]
    fn read_byte(&mut self) -> Option<u8> {
        if self.is_ready_to_read() {
            Some(self._read(Register::DATA))
        } else {
            None
        }
    }
}

use super::{gpio::*, mbox::*, *};
use crate::{Hal, HalCpu, HalTrait, mem::mmio::Mmio32};
use core::fmt;

#[allow(dead_code)]
static mut UART0: Uart0 = Uart0::CR;

/// Uart 0 (PL011)
#[allow(dead_code)]
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
pub enum Uart0 {
    DR = 0x00,
    RSRECR = 0x04,
    FR = 0x18,
    ILPR = 0x20,
    IBRD = 0x24,
    FBRD = 0x28,
    LCRH = 0x2c,
    CR = 0x30,
    IFLS = 0x34,
    IMSC = 0x38,
    RIS = 0x3c,
    MIS = 0x40,
    ICR = 0x44,
    DMACR = 0x48,
    ITCR = 0x80,
    ITIP = 0x84,
    ITOP = 0x88,
    TDR = 0x8c,
}

unsafe impl Mmio32 for Uart0 {
    #[inline]
    fn addr(&self) -> usize {
        super::mmio_base() + 0x20_1000 + *self as usize
    }
}

#[allow(dead_code)]
impl Uart0 {
    #[inline]
    pub fn shared<'a>() -> &'a mut Self {
        unsafe { &mut *(&raw mut UART0) }
    }

    #[inline(never)]
    pub fn init() -> Result<&'static mut Self, ()> {
        unsafe {
            // Disable UART0.
            Uart0::CR.write(0);

            Gpio::UART0_TXD.use_as_alt0();
            Gpio::UART0_RXD.use_as_alt0();
            Gpio::enable_pins(&[Gpio::UART0_TXD, Gpio::UART0_RXD], Pull::NONE);

            // Clear pending interrupts.
            Uart0::ICR.write(0x7ff);

            let mut mbox = Mbox::PROP.new::<10>();
            mbox.append(Tag::SetClockRate(ClockId::UART, 3000000, 0))?;
            mbox.call()?;

            // Divider = 3000000 / (16 * 115200) = 1.627 = ~1.
            Uart0::IBRD.write(1);
            // Fractional part register = (.627 * 64) + 0.5 = 40.6 = ~40.
            Uart0::FBRD.write(40);

            // Enable FIFO & 8 bit data transmission (1 stop bit, no parity).
            Uart0::LCRH.write(0x0070);

            // Mask all interrupts.
            Uart0::IMSC.write(0x7f2);

            // Enable UART0, receive & transfer part of UART.
            Uart0::CR.write(0x301);
        }
        Ok(Self::shared())
    }

    #[inline]
    fn is_output_ready(&mut self) -> bool {
        unsafe { (Uart0::FR.read() & 0x20) == 0 }
    }

    #[inline]
    fn is_input_ready(&mut self) -> bool {
        unsafe { (Uart0::FR.read() & 0x10) == 0 }
    }

    fn write_byte(&mut self, ch: u8) {
        while !self.is_output_ready() {
            Hal::cpu().no_op();
        }
        unsafe {
            Uart0::DR.write(ch as u32);
        }
    }

    fn read_byte(&mut self) -> u8 {
        while !self.is_input_ready() {
            Hal::cpu().no_op();
        }
        unsafe { Uart0::DR.read() as u8 }
    }
}

impl fmt::Write for Uart0 {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for ch in s.bytes() {
            self.write_byte(ch);
        }
        Ok(())
    }
}

impl SimpleTextOutput for Uart0 {
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

impl SimpleTextInput for Uart0 {
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

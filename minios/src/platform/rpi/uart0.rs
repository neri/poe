use super::{gpio::*, mbox::*, *};
use crate::{Hal, HalCpu, HalTrait, mem::mmio::Mmio32, vt100::VT100};

#[allow(dead_code)]
static mut UART0: Uart0 = Uart0::CR;

static mut SHARED: UnsafeCell<VT100> = UnsafeCell::new(VT100::new(Uart0::shared_raw()));

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
    pub const fn shared_raw<'a>() -> &'a mut Self {
        unsafe { &mut *(&raw mut UART0) }
    }

    #[inline]
    pub const fn shared() -> &'static mut VT100<'static> {
        unsafe { &mut *(&raw mut SHARED) }.get_mut()
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
        Ok(Self::shared_raw())
    }

    #[inline]
    fn is_output_ready(&mut self) -> bool {
        unsafe { (Uart0::FR.read() & 0x20) == 0 }
    }

    #[inline]
    fn is_input_ready(&mut self) -> bool {
        unsafe { (Uart0::FR.read() & 0x10) == 0 }
    }
}

impl SerialIo for Uart0 {
    #[inline]
    fn reset(&mut self) {
        //
    }

    #[inline]
    fn write_byte(&mut self, byte: u8) {
        while !self.is_output_ready() {
            Hal::cpu().no_op();
        }
        unsafe {
            Uart0::DR.write(byte as u32);
        }
    }

    #[inline]
    fn read_byte(&mut self) -> Option<u8> {
        if self.is_input_ready() {
            Some(unsafe { Uart0::DR.read() as u8 })
        } else {
            None
        }
    }
}

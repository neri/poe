use super::{gpio::*, *};
use crate::{Hal, HalCpu, HalTrait, mem::mmio::Mmio32};

#[allow(dead_code)]
static mut UART1: MiniUart = MiniUart {};

/// Mini UART
#[allow(dead_code)]
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
enum Uart1 {
    ENABLE = 0x0004,
    IO = 0x0040,
    IER = 0x0044,
    IIR = 0x0048,
    LCR = 0x004c,
    MCR = 0x0050,
    LSR = 0x0054,
    MSR = 0x0058,
    SCRATCH = 0x005c,
    CNTL = 0x0060,
    STAT = 0x0064,
    BAUD = 0x0068,
}

unsafe impl Mmio32 for Uart1 {
    #[inline]
    fn addr(&self) -> usize {
        super::mmio_base() + 0x21_5000 + *self as usize
    }
}

#[allow(dead_code)]
pub struct MiniUart;

#[allow(dead_code)]
impl MiniUart {
    pub const CLOCK: u32 = 500_000_000;

    #[inline]
    pub fn shared<'a>() -> &'a mut MiniUart {
        unsafe { &mut *(&raw mut UART1) }
    }

    #[inline]
    pub const fn baud(baud: u32) -> u32 {
        match Self::CLOCK.checked_div(baud * 8) {
            Some(v) => v - 1,
            None => 0,
        }
    }

    pub fn init() -> Result<(), ()> {
        unsafe {
            Gpio::UART0_TXD.use_as_alt5();
            Gpio::UART0_RXD.use_as_alt5();
            Gpio::enable_pins(&[Gpio::UART0_TXD, Gpio::UART0_RXD], Pull::NONE);

            Uart1::ENABLE.write(1); //enable UART1, AUX mini uart
            Uart1::CNTL.write(0);
            Uart1::LCR.write(3); //8 bits
            Uart1::MCR.write(0);
            Uart1::IER.write(0);
            Uart1::IIR.write(0xc6); //disable interrupts

            match current_machine_type() {
                MachineType::Unknown => {
                    //
                }
                MachineType::RaspberryPi3 => {
                    Uart1::BAUD.write(270);
                }
                MachineType::RaspberryPi4 => {
                    Uart1::BAUD.write(Self::baud(115200));
                }
                MachineType::RaspberryPi5 => {
                    // TODO:
                    todo!()
                }
            }

            Uart1::CNTL.write(3); //enable RX/TX
        }

        Ok(())
    }

    #[inline]
    pub fn is_output_ready(&self) -> bool {
        (unsafe { Uart1::LSR.read() } & 0x20) != 0
    }

    #[inline]
    pub fn is_input_ready(&self) -> bool {
        (unsafe { Uart1::LSR.read() } & 0x01) != 0
    }

    pub fn write_byte(&self, ch: u8) {
        while !self.is_output_ready() {
            Hal::cpu().no_op();
        }
        unsafe {
            Uart1::IO.write(ch as u32);
        }
    }

    pub fn read_byte(&self) -> u8 {
        while !self.is_input_ready() {
            Hal::cpu().no_op();
        }
        unsafe { Uart1::IO.read() as u8 }
    }
}

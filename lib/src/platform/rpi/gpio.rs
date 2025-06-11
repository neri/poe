use crate::{mem::mmio::*, *};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum Gpio {
    Pin00 = 0,
    Pin01,
    Pin02,
    Pin03,
    Pin04,
    Pin05,
    Pin06,
    Pin07,
    Pin08,
    Pin09,
    Pin10,
    Pin11,
    Pin12,
    Pin13,
    Pin14,
    Pin15,
    Pin16,
    Pin17,
    Pin18,
    Pin19,
    Pin20,
    Pin21,
    Pin22,
    Pin23,
    Pin24,
    Pin25,
    Pin26,
    Pin27,
    Pin28,
    Pin29,
    Pin30,
    Pin31,
    Pin32,
    Pin33,
    Pin34,
    Pin35,
    Pin36,
    Pin37,
    Pin38,
    Pin39,
    Pin40,
    Pin41,
    Pin42,
    Pin43,
    Pin44,
    Pin45,
    Pin46,
    Pin47,
    Pin48,
    Pin49,
    Pin50,
    Pin51,
    Pin52,
    Pin53,
    Pin54,
    Pin55,
    Pin56,
    Pin57,
}

#[allow(dead_code)]
impl Gpio {
    pub const SDA1: Self = Self::Pin02;
    pub const SCL1: Self = Self::Pin03;

    pub const SPI0_CE1_N: Self = Self::Pin07;
    pub const SPI0_CE0_N: Self = Self::Pin08;
    pub const SPI0_MISO: Self = Self::Pin09;
    pub const SPI0_MOSI: Self = Self::Pin10;
    pub const SPI0_SCLK: Self = Self::Pin11;

    pub const UART0_TXD: Self = Self::Pin14;
    pub const UART0_RXD: Self = Self::Pin15;

    #[inline]
    pub fn init(&self, pull: Pull, function: Function) {
        self.pull(pull);
        self.function(function);
    }

    #[inline]
    pub fn set(&self) {
        GpioRegs::set(*self);
    }

    #[inline]
    pub fn clear(&self) {
        GpioRegs::clear(*self);
    }

    #[inline]
    pub fn pull(&self, pull: Pull) {
        GpioRegs::pull(*self, pull);
    }

    #[inline]
    pub fn function(&self, function: Function) {
        GpioRegs::function(*self, function);
    }

    #[inline]
    pub fn use_as_alt0(&self) {
        self.init(Pull::NONE, Function::ALT0);
    }

    #[inline]
    pub fn use_as_alt3(&self) {
        self.init(Pull::NONE, Function::ALT3);
    }

    #[inline]
    pub fn use_as_alt5(&self) {
        self.init(Pull::NONE, Function::ALT5);
    }

    #[inline]
    pub fn set_output(&self, val: bool) {
        if val {
            self.set();
        } else {
            self.clear();
        }
    }

    #[inline]
    pub fn enable_pins(pins: &[Self], pull: Pull) {
        GpioRegs::enable_pins(pins, pull);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pull {
    NONE = 0,
    DOWN = 1,
    UP = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Function {
    INPUT = 0,
    OUTPUT = 1,
    ALT0 = 4,
    ALT1 = 5,
    ALT2 = 6,
    ALT3 = 7,
    ALT4 = 3,
    ALT5 = 2,
}

#[allow(dead_code)]
#[repr(usize)]
#[derive(Debug, Clone, Copy)]
enum GpioRegs {
    GPFSEL0 = 0x0000,
    GPFSEL1 = 0x0004,
    GPFSEL2 = 0x0008,
    GPFSEL3 = 0x000c,
    GPFSEL4 = 0x0010,
    GPFSEL5 = 0x0014,
    GPSET0 = 0x001c,
    GPSET1 = 0x0020,
    GPCLR0 = 0x0028,
    GPLEV0 = 0x0034,
    GPLEV1 = 0x0038,
    GPEDS0 = 0x0040,
    GPEDS1 = 0x0044,
    GPREN0 = 0x004c,
    GPREN1 = 0x0050,
    GPFEN0 = 0x0058,
    GPFEN1 = 0x005c,
    GPHEN0 = 0x0064,
    GPHEN1 = 0x0068,
    GPLEN0 = 0x0070,
    GPLEN1 = 0x0074,
    GPPUD = 0x0094,
    GPPUDCLK0 = 0x0098,
    GPPUDCLK1 = 0x009c,
    GPPUPPDN0 = 0x00e4,
    GPPUPPDN1 = 0x00e8,
    GPPUPPDN2 = 0x00ec,
    GPPUPPDN3 = 0x00f0,
}

#[allow(dead_code)]
impl GpioRegs {
    #[inline]
    unsafe fn base_addr(&self) -> usize {
        super::mmio_base() + 0x0020_0000 + *self as usize
    }

    #[inline]
    unsafe fn as_reg(&self) -> Mmio32Reg {
        unsafe { Mmio32Reg(self.base_addr()) }
    }

    #[inline]
    fn enable_pins(pins: &[Gpio], pull: Pull) {
        let acc = pins.iter().fold(0u64, |a, v| a | (1 << *v as usize));
        let acc0 = acc as u32;
        let acc1 = (acc >> 32) as u32;
        unsafe {
            if acc0 > 0 {
                Self::GPPUD.as_reg().write(pull as u32);
                for _ in 0..150 {
                    Hal::cpu().no_op();
                }
                Self::GPPUDCLK0.as_reg().write(acc0);
                for _ in 0..150 {
                    Hal::cpu().no_op();
                }
                Self::GPPUDCLK0.as_reg().write(0);
            }
            if acc1 > 0 {
                Self::GPPUD.as_reg().write(pull as u32);
                for _ in 0..150 {
                    Hal::cpu().no_op();
                }
                Self::GPPUDCLK1.as_reg().write(acc1);
                for _ in 0..150 {
                    Hal::cpu().no_op();
                }
                Self::GPPUDCLK1.as_reg().write(0);
            }
        }
    }

    #[inline]
    fn set(pin: Gpio) {
        unsafe { Self::GPSET0._gpio_call(pin, 1, 1) }
    }

    #[inline]
    fn clear(pin: Gpio) {
        unsafe { Self::GPCLR0._gpio_call(pin, 1, 1) }
    }

    #[inline]
    fn pull(pin: Gpio, pull: Pull) {
        unsafe { Self::GPPUPPDN0._gpio_call(pin, pull as u32, 2) }
    }

    #[inline]
    fn function(pin: Gpio, function: Function) {
        unsafe { Self::GPFSEL0._gpio_call(pin, function as u32, 3) }
    }

    unsafe fn _gpio_call(&self, pin: Gpio, value: u32, field_size: usize) {
        unsafe {
            let pin_number = pin as usize;
            let field_mask = (1 << field_size) - 1;
            let num_fields = 32 / field_size;
            let shift = (pin_number % num_fields) * field_size;
            let reg = Mmio32Reg(self.base_addr() + ((pin_number / num_fields) * 4));

            let mut temp = reg.read();
            temp &= !(field_mask << shift);
            temp |= value << shift;
            reg.write(temp);
        }
    }
}

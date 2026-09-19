//! Rockchip SPI controller (polled, 8-bit frames)
//!
//! Only the transfer mode and the number of frames are programmed.
//! The clock, the SPI mode and the pins are left as configured by the firmware,
//! the same as depthcharge does.

use crate::platform::arm64dt::counter_us;
use crate::platform::arm64dt::spi::{SpiDevice, SpiTimeout};

/// A chip select of the Rockchip SPI controller
pub struct RkSpi {
    base: usize,
    cs: u32,
}

impl RkSpi {
    pub const COMPATIBLE: &str = "rockchip,rk3066-spi";

    const CTRLR0: usize = 0x0000;
    const CTRLR1: usize = 0x0004;
    const ENR: usize = 0x0008;
    const SER: usize = 0x000c;
    const RXFLR: usize = 0x0020;
    const SR: usize = 0x0024;
    const TXDR: usize = 0x0400;
    const RXDR: usize = 0x0800;

    /// 8-bit APB access
    const CTRLR0_HALF_WORD_TX: u32 = 1 << 13;
    const CTRLR0_TMOD_SHIFT: u32 = 18;
    const CTRLR0_TMOD_MASK: u32 = 3 << Self::CTRLR0_TMOD_SHIFT;
    const TMOD_TX_ONLY: u32 = 1;
    const TMOD_RX_ONLY: u32 = 2;

    const SR_BUSY: u32 = 1 << 0;
    const SR_TF_FULL: u32 = 1 << 1;

    const RXFLR_MASK: u32 = 0x3f;
    const MAX_FRAMES: usize = 0xfffe;

    const TIMEOUT_US: u64 = 100_000;

    /// The chip select `cs` of the controller at `base`
    #[inline]
    pub const fn new(base: usize, cs: u32) -> Self {
        Self { base, cs }
    }

    unsafe fn begin(&self, tmod: u32, frames: usize) {
        unsafe {
            self.write_reg(Self::ENR, 0);
            let ctrlr0 = (self.read_reg(Self::CTRLR0) & !Self::CTRLR0_TMOD_MASK)
                | Self::CTRLR0_HALF_WORD_TX
                | (tmod << Self::CTRLR0_TMOD_SHIFT);
            self.write_reg(Self::CTRLR0, ctrlr0);
            self.write_reg(Self::CTRLR1, (frames - 1) as u32);
            self.write_reg(Self::ENR, 1);
        }
    }

    unsafe fn end(&self) -> Result<(), SpiTimeout> {
        unsafe {
            let start = counter_us();
            while (self.read_reg(Self::SR) & Self::SR_BUSY) != 0 {
                self.check_timeout(start)?;
            }
            self.write_reg(Self::ENR, 0);
        }
        Ok(())
    }

    /// Disables the controller and returns `Err` if the timeout has expired.
    unsafe fn check_timeout(&self, start: u64) -> Result<(), SpiTimeout> {
        if counter_us().wrapping_sub(start) > Self::TIMEOUT_US {
            unsafe { self.write_reg(Self::ENR, 0) };
            Err(SpiTimeout)
        } else {
            core::hint::spin_loop();
            Ok(())
        }
    }

    #[inline]
    unsafe fn read_reg(&self, offset: usize) -> u32 {
        unsafe { ((self.base + offset) as *const u32).read_volatile() }
    }

    #[inline]
    unsafe fn write_reg(&self, offset: usize, value: u32) {
        unsafe { ((self.base + offset) as *mut u32).write_volatile(value) }
    }
}

impl SpiDevice for RkSpi {
    unsafe fn select(&self) {
        unsafe { self.write_reg(Self::SER, 1 << self.cs) };
    }

    /// Deasserts all chip selects.
    unsafe fn deselect(&self) {
        unsafe { self.write_reg(Self::SER, 0) };
    }

    unsafe fn write(&self, data: &[u8]) -> Result<(), SpiTimeout> {
        if data.is_empty() {
            return Ok(());
        }
        assert!(data.len() <= Self::MAX_FRAMES);
        unsafe {
            self.begin(Self::TMOD_TX_ONLY, data.len());
            let start = counter_us();
            for &byte in data {
                while (self.read_reg(Self::SR) & Self::SR_TF_FULL) != 0 {
                    self.check_timeout(start)?;
                }
                self.write_reg(Self::TXDR, byte as u32);
            }
            self.end()
        }
    }

    unsafe fn read(&self, buf: &mut [u8]) -> Result<(), SpiTimeout> {
        if buf.is_empty() {
            return Ok(());
        }
        assert!(buf.len() <= Self::MAX_FRAMES);
        unsafe {
            self.begin(Self::TMOD_RX_ONLY, buf.len());
            let start = counter_us();
            let mut index = 0;
            while index < buf.len() {
                let level = self.read_reg(Self::RXFLR) & Self::RXFLR_MASK;
                if level == 0 {
                    self.check_timeout(start)?;
                    continue;
                }
                for _ in 0..level {
                    let byte = self.read_reg(Self::RXDR) as u8;
                    if index < buf.len() {
                        buf[index] = byte;
                        index += 1;
                    }
                }
            }
            self.end()
        }
    }
}

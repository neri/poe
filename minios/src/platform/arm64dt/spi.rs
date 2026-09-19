//! SPI device (a chip select of an SPI controller)
//!
//! Implemented by the SoC's SPI controller drivers, used by the device drivers (e.g. the ChromeOS EC).

/// The SPI controller did not finish in time
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpiTimeout;

/// SPI device on a controller set up by the firmware (clock, SPI mode, pins)
pub trait SpiDevice {
    /// Asserts the chip select of the device.
    unsafe fn select(&self);

    /// Deasserts the chip select.
    unsafe fn deselect(&self);

    /// Transmits `data` (the received data is discarded).
    unsafe fn write(&self, data: &[u8]) -> Result<(), SpiTimeout>;

    /// Receives `buf.len()` bytes.
    unsafe fn read(&self, buf: &mut [u8]) -> Result<(), SpiTimeout>;
}

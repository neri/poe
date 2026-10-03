//! Polled PIO driver for the SD Host Controller Interface register set.

use super::{BusWidth, DataDirection, Host, HostError, Response, ResponseType};
use crate::System;

const PRESENT_STATE: usize = 0x24;
const HOST_CONTROL: usize = 0x28;
const POWER_CONTROL: usize = 0x29;
const CLOCK_CONTROL: usize = 0x2c;
const SOFTWARE_RESET: usize = 0x2f;
const TIMEOUT_CONTROL: usize = 0x2e;
const INT_STATUS: usize = 0x30;
const ARGUMENT: usize = 0x08;
const COMMAND: usize = 0x0e;
const RESPONSE: usize = 0x10;
const BLOCK_SIZE: usize = 0x04;
const BLOCK_COUNT: usize = 0x06;
const TRANSFER_MODE: usize = 0x0c;
const BUFFER: usize = 0x20;
// Synopsys DWC MSHC vendor area 1 pointer and CV18xx-specific registers.
const VENDOR_AREA1_POINTER: usize = 0xe8;
const VENDOR_AREA1_POINTER_MASK: u32 = 0x0fff;
const CV18XX_MSHC_CTRL: usize = 0x00;
const CV18XX_PHY_TX_RX_DLY: usize = 0x40;
const CV18XX_PHY_CONFIG: usize = 0x4c;

const CMD_INHIBIT: u32 = 1 << 0;
const DATA_INHIBIT: u32 = 1 << 1;
// SDHCI Present State: Buffer Read Enable and Buffer Write Enable.
const READ_ENABLE: u32 = 1 << 11;
const WRITE_ENABLE: u32 = 1 << 10;
const INT_COMMAND_COMPLETE: u32 = 1 << 0;
const INT_TRANSFER_COMPLETE: u32 = 1 << 1;
const INT_BUFFER_WRITE_READY: u32 = 1 << 4;
const INT_BUFFER_READ_READY: u32 = 1 << 5;
const INT_ERROR: u32 = 1 << 15;
const INT_ENABLE_MASK: u32 = INT_COMMAND_COMPLETE
    | INT_TRANSFER_COMPLETE
    | INT_BUFFER_WRITE_READY
    | INT_BUFFER_READ_READY
    | INT_ERROR
    | (0x7f << 16); // command/data timeout, CRC, end-bit and index errors
const INT_STATUS_ALL: u32 = 0xffff_ffff;

/// One SDHCI-compatible controller. The caller supplies its input clock and
/// the platform's direct monotonic microsecond counter.
pub struct Sdhci {
    base: usize,
    input_clock_hz: u32,
    now_us: fn() -> u64,
    cv18xx_vendor_area1: Option<usize>,
    prepared: Option<(DataDirection, u16, u16)>,
    bcm2835_32bit: bool,
    card_clock_hz: u32,
}

impl Sdhci {
    /// # Safety
    /// `base` must point to a mapped SDHCI register bank for this controller.
    pub unsafe fn new(base: usize, input_clock_hz: u32, now_us: fn() -> u64) -> Self {
        Self {
            base,
            input_clock_hz,
            now_us,
            cv18xx_vendor_area1: None,
            prepared: None,
            bcm2835_32bit: false,
            card_clock_hz: 0,
        }
    }

    /// BCM2835 Arasan requires aligned 32-bit accesses and combined writes
    /// of block size/count and transfer mode/command (clock-domain crossing).
    ///
    /// # Safety
    /// `base` must point to the mapped BCM2835 Arasan SDHCI register bank.
    pub unsafe fn new_bcm2835(base: usize, input_clock_hz: u32, now_us: fn() -> u64) -> Self {
        let mut host = unsafe { Self::new(base, input_clock_hz, now_us) };
        host.bcm2835_32bit = true;
        host
    }

    /// Creates a host for the Sophgo CV18xx DWC MSHC, including its vendor PHY.
    ///
    /// # Safety
    /// `base` must point to a mapped SDHCI register bank of at least
    /// `register_size` bytes. The CV18xx vendor area pointer must identify a
    /// mapped register range inside that bank.
    pub unsafe fn new_cv18xx(
        base: usize,
        register_size: usize,
        input_clock_hz: u32,
        now_us: fn() -> u64,
    ) -> Result<Self, HostError> {
        if register_size < VENDOR_AREA1_POINTER + 4 {
            return Err(HostError::Unsupported);
        }
        let pointer = unsafe { ((base + VENDOR_AREA1_POINTER) as *const u32).read_volatile() };
        let area1 = (pointer & VENDOR_AREA1_POINTER_MASK) as usize;
        if area1 & 3 != 0
            || area1
                .checked_add(CV18XX_PHY_CONFIG + 4)
                .is_none_or(|end| end > register_size)
        {
            return Err(HostError::Unsupported);
        }
        Ok(Self {
            base,
            input_clock_hz,
            now_us,
            cv18xx_vendor_area1: Some(area1),
            prepared: None,
            bcm2835_32bit: false,
            card_clock_hz: 0,
        })
    }

    fn read8(&self, offset: usize) -> u8 {
        if self.bcm2835_32bit {
            (self.read32(offset & !3) >> ((offset & 3) * 8)) as u8
        } else {
            unsafe { ((self.base + offset) as *const u8).read_volatile() }
        }
    }

    fn read16(&self, offset: usize) -> u16 {
        if self.bcm2835_32bit {
            (self.read32(offset & !3) >> ((offset & 3) * 8)) as u16
        } else {
            unsafe { ((self.base + offset) as *const u16).read_volatile() }
        }
    }

    fn read32(&self, offset: usize) -> u32 {
        unsafe { ((self.base + offset) as *const u32).read_volatile() }
    }

    fn write8(&self, offset: usize, value: u8) {
        if self.bcm2835_32bit {
            let shift = (offset & 3) * 8;
            let previous = self.read32(offset & !3);
            self.write32(
                offset & !3,
                (previous & !(0xff << shift)) | ((value as u32) << shift),
            );
        } else {
            unsafe { ((self.base + offset) as *mut u8).write_volatile(value) }
        }
    }

    fn write16(&self, offset: usize, value: u16) {
        if self.bcm2835_32bit {
            let shift = (offset & 3) * 8;
            let previous = self.read32(offset & !3);
            self.write32(
                offset & !3,
                (previous & !(0xffff << shift)) | ((value as u32) << shift),
            );
        } else {
            unsafe { ((self.base + offset) as *mut u16).write_volatile(value) }
        }
    }

    fn write32(&self, offset: usize, value: u32) {
        unsafe { ((self.base + offset) as *mut u32).write_volatile(value) }
        // Allow four SD clocks for register writes to cross into the card
        // clock domain, rounded up to a microsecond. FIFO is exempt.
        if self.bcm2835_32bit && offset != BUFFER {
            let delay = if self.card_clock_hz == 0 {
                10
            } else {
                4_000_000u64.div_ceil(self.card_clock_hz as u64)
            };
            let started = (self.now_us)();
            while (self.now_us)().wrapping_sub(started) < delay {
                core::hint::spin_loop();
            }
        }
    }

    fn configure_cv18xx_phy(&self) {
        let Some(area1) = self.cv18xx_vendor_area1 else {
            return;
        };
        let mshc_ctrl = self.read32(area1 + CV18XX_MSHC_CTRL);
        self.write32(area1 + CV18XX_MSHC_CTRL, mshc_ctrl | (1 << 1)); // latency_1t
        let phy_config = self.read32(area1 + CV18XX_PHY_CONFIG);
        self.write32(area1 + CV18XX_PHY_CONFIG, phy_config | 1); // TX_BPS
        // TX source: inverted TX clock; RX source: inverted RX clock, zero tap.
        self.write32(area1 + CV18XX_PHY_TX_RX_DLY, (1 << 8) | (1 << 24));
    }

    fn log_cv18xx_command_timeout(&self, index: u8, phase: &str) {
        let Some(area1) = self.cv18xx_vendor_area1 else {
            return;
        };
        crate::println!(
            "CV18xx SD: CMD{} {} timeout: int={:08x} present={:08x} clock={:04x} power={:02x} cmd={:04x} arg={:08x}",
            index,
            phase,
            self.read32(INT_STATUS),
            self.read32(PRESENT_STATE),
            self.read16(CLOCK_CONTROL),
            self.read8(POWER_CONTROL),
            self.read16(COMMAND),
            self.read32(ARGUMENT)
        );
        crate::println!(
            "CV18xx SD: vendor ctrl={:08x} txrx={:08x} phy={:08x}",
            self.read32(area1 + CV18XX_MSHC_CTRL),
            self.read32(area1 + CV18XX_PHY_TX_RX_DLY),
            self.read32(area1 + CV18XX_PHY_CONFIG)
        );
    }

    fn wait_status(&self, mask: u32, deadline: u64) -> Result<u32, HostError> {
        loop {
            let status = self.read32(INT_STATUS);
            if status & INT_ERROR != 0 {
                if self.cv18xx_vendor_area1.is_some() {
                    crate::println!(
                        "CV18xx SD: controller interrupt error: status={:08x} present={:08x}",
                        status,
                        self.read32(PRESENT_STATE)
                    );
                }
                self.write32(INT_STATUS, status);
                return Err(if status & (1 << 16) != 0 {
                    HostError::Timeout
                } else if status & ((1 << 17) | (1 << 21)) != 0 {
                    HostError::Crc
                } else {
                    HostError::Io
                });
            }
            if status & mask != 0 {
                self.write32(INT_STATUS, status & mask);
                return Ok(status);
            }
            if reached((self.now_us)(), deadline) {
                return Err(HostError::Timeout);
            }
            core::hint::spin_loop();
        }
    }

    fn wait_present_clear(&self, mask: u32, deadline: u64) -> Result<(), HostError> {
        loop {
            if self.read32(PRESENT_STATE) & mask == 0 {
                return Ok(());
            }
            if reached((self.now_us)(), deadline) {
                return Err(HostError::Timeout);
            }
            core::hint::spin_loop();
        }
    }

    fn command_flags(response: ResponseType, data: bool) -> u16 {
        let response = match response {
            ResponseType::None => 0,
            ResponseType::R2 => (1 << 0) | (1 << 3),
            ResponseType::R3 => 2 << 0,
            ResponseType::R1b => (3 << 0) | (1 << 3) | (1 << 4),
            ResponseType::R1 | ResponseType::R6 | ResponseType::R7 => {
                (2 << 0) | (1 << 3) | (1 << 4)
            }
        };
        response | if data { 1 << 5 } else { 0 }
    }

    fn response(&self, kind: ResponseType) -> Response {
        match kind {
            ResponseType::None => Response::default(),
            ResponseType::R2 => {
                let a = self.read32(RESPONSE);
                let b = self.read32(RESPONSE + 4);
                let c = self.read32(RESPONSE + 8);
                let d = self.read32(RESPONSE + 12);
                Response {
                    short: 0,
                    long: [
                        (d << 8) | (c >> 24),
                        (c << 8) | (b >> 24),
                        (b << 8) | (a >> 24),
                        a << 8,
                    ],
                }
            }
            _ => Response {
                short: self.read32(RESPONSE),
                long: [0; 4],
            },
        }
    }

    fn transfer_words(
        &self,
        bytes: &mut [u8],
        direction: DataDirection,
        deadline: u64,
    ) -> Result<(), HostError> {
        let ready_bit = match direction {
            DataDirection::Read => INT_BUFFER_READ_READY,
            DataDirection::Write => INT_BUFFER_WRITE_READY,
        };
        let access_bit = match direction {
            DataDirection::Read => READ_ENABLE,
            DataDirection::Write => WRITE_ENABLE,
        };
        self.wait_status(ready_bit, deadline)?;
        if self.read32(PRESENT_STATE) & access_bit == 0 {
            if self.cv18xx_vendor_area1.is_some() {
                crate::println!(
                    "CV18xx SD: buffer-ready status without present-state enable: direction={:?} present={:08x} int={:08x}",
                    direction,
                    self.read32(PRESENT_STATE),
                    self.read32(INT_STATUS)
                );
            }
            return Err(HostError::Io);
        }
        let mut offset = 0;
        while offset < bytes.len() {
            let end = (offset + 4).min(bytes.len());
            match direction {
                DataDirection::Read => {
                    let word = self.read32(BUFFER).to_le_bytes();
                    bytes[offset..end].copy_from_slice(&word[..end - offset]);
                }
                DataDirection::Write => {
                    let mut word = [0u8; 4];
                    word[..end - offset].copy_from_slice(&bytes[offset..end]);
                    self.write32(BUFFER, u32::from_le_bytes(word));
                }
            }
            offset = end;
        }
        self.wait_status(INT_TRANSFER_COMPLETE, deadline)?;
        Ok(())
    }
}

impl Host for Sdhci {
    fn now_us(&self) -> u64 {
        (self.now_us)()
    }

    fn reset(&mut self) -> Result<(), HostError> {
        self.card_clock_hz = 0;
        self.write8(SOFTWARE_RESET, 1);
        let deadline = self.now_us().wrapping_add(100_000);
        loop {
            if self.read8(SOFTWARE_RESET) & 1 == 0 {
                break;
            }
            if reached(self.now_us(), deadline) {
                return Err(HostError::Timeout);
            }
            core::hint::spin_loop();
        }
        self.write32(INT_STATUS, INT_STATUS_ALL);
        // Polling still depends on status bits being enabled. Signal enables
        // remain clear, so this does not require or generate interrupts.
        self.write32(INT_STATUS + 4, INT_ENABLE_MASK);
        // SDHCI: voltage select 111 = 3.3 V, bit 0 = bus power on.
        self.write8(POWER_CONTROL, 0x0f);
        // Use the largest non-reserved data/busy timeout exponent. Software
        // deadlines still bound every poll; reset's exponent 0 is too short
        // for ordinary data transfers when the clock is increased.
        self.write8(TIMEOUT_CONTROL, 0x0e);
        self.configure_cv18xx_phy();
        self.prepared = None;
        Ok(())
    }

    fn set_clock(&mut self, hz: u32) -> Result<u32, HostError> {
        if hz == 0 || self.input_clock_hz == 0 {
            return Err(HostError::Unsupported);
        }
        let mut divisor = 1u32;
        while self.input_clock_hz / divisor > hz && divisor < 1024 {
            divisor <<= 1;
        }
        if divisor > 1024 || self.input_clock_hz / divisor > hz {
            return Err(HostError::Unsupported);
        }
        let divider = divisor / 2;
        let encoded_divider = ((divider & 0xff) << 8) | ((divider & 0x300) >> 2);
        self.write16(CLOCK_CONTROL, 0);
        self.write16(CLOCK_CONTROL, encoded_divider as u16 | 1);
        let deadline = self.now_us().wrapping_add(100_000);
        loop {
            if self.read16(CLOCK_CONTROL) & (1 << 1) != 0 {
                break;
            }
            if reached(self.now_us(), deadline) {
                return Err(HostError::Timeout);
            }
            core::hint::spin_loop();
        }
        self.write16(CLOCK_CONTROL, encoded_divider as u16 | 7);
        self.card_clock_hz = self.input_clock_hz / divisor;
        Ok(self.card_clock_hz)
    }

    fn set_bus_width(&mut self, width: BusWidth) -> Result<(), HostError> {
        let mut control = self.read8(HOST_CONTROL);
        match width {
            BusWidth::One => control &= !(1 << 1),
            BusWidth::Four => control |= 1 << 1,
        }
        self.write8(HOST_CONTROL, control);
        Ok(())
    }

    fn prepare_data(
        &mut self,
        direction: DataDirection,
        block_size: u16,
        blocks: u16,
    ) -> Result<(), HostError> {
        if self.prepared.is_some() || block_size == 0 || blocks == 0 || blocks > 1 {
            return Err(HostError::Unsupported);
        }
        self.wait_present_clear(CMD_INHIBIT | DATA_INHIBIT, self.now_us() + 100_000)?;
        if self.bcm2835_32bit {
            self.write32(
                BLOCK_SIZE,
                (block_size as u32 & 0x0fff) | ((blocks as u32) << 16),
            );
            // Defer TRANSFER_MODE until COMMAND; a read-modify-write of 0x0c
            // would otherwise reissue the previous command on this hardware.
        } else {
            self.write16(BLOCK_SIZE, block_size & 0x0fff);
            self.write16(BLOCK_COUNT, blocks);
            self.write16(TRANSFER_MODE, transfer_mode(direction));
        }
        self.prepared = Some((direction, block_size, blocks));
        Ok(())
    }

    fn command(
        &mut self,
        index: u8,
        argument: u32,
        response: ResponseType,
        deadline_us: u64,
    ) -> Result<Response, HostError> {
        let data = self.prepared.is_some();
        if let Err(error) = self.wait_present_clear(
            CMD_INHIBIT | if data { DATA_INHIBIT } else { 0 },
            deadline_us,
        ) {
            if error == HostError::Timeout {
                self.log_cv18xx_command_timeout(index, "command-inhibit");
                return Err(HostError::CommandInhibitTimeout);
            }
            return Err(error);
        }
        self.write32(INT_STATUS, INT_STATUS_ALL);
        self.write32(ARGUMENT, argument);
        let command = Self::command_flags(response, data) | ((index as u16) << 8);
        if self.bcm2835_32bit {
            let mode = self
                .prepared
                .map_or(0, |(direction, _, _)| transfer_mode(direction));
            self.write32(TRANSFER_MODE, (mode as u32) | ((command as u32) << 16));
        } else {
            self.write16(COMMAND, command);
        }
        if let Err(error) = self.wait_status(INT_COMMAND_COMPLETE, deadline_us) {
            if error == HostError::Timeout {
                self.log_cv18xx_command_timeout(index, "command-complete");
                return Err(HostError::CommandCompleteTimeout);
            }
            return Err(error);
        }
        let result = self.response(response);
        if response == ResponseType::R1b {
            self.wait_present_clear(DATA_INHIBIT, deadline_us)?;
        }
        Ok(result)
    }

    fn read_data(&mut self, bytes: &mut [u8], deadline_us: u64) -> Result<(), HostError> {
        let Some((DataDirection::Read, block_size, blocks)) = self.prepared.take() else {
            return Err(HostError::Io);
        };
        if bytes.len() != block_size as usize * blocks as usize {
            return Err(HostError::Io);
        }
        self.transfer_words(bytes, DataDirection::Read, deadline_us)
    }

    fn write_data(&mut self, bytes: &[u8], deadline_us: u64) -> Result<(), HostError> {
        let Some((DataDirection::Write, block_size, blocks)) = self.prepared.take() else {
            return Err(HostError::Io);
        };
        if bytes.len() != block_size as usize * blocks as usize {
            return Err(HostError::Io);
        }
        let mut data = [0u8; 4];
        // Feed the host FIFO directly, avoiding a second 512-byte stack buffer.
        let _ = &mut data;
        let ready_bit = INT_BUFFER_WRITE_READY;
        if self.read32(PRESENT_STATE) & WRITE_ENABLE == 0 {
            return Err(HostError::Io);
        }
        self.wait_status(ready_bit, deadline_us)?;
        for chunk in bytes.chunks(4) {
            data.fill(0);
            data[..chunk.len()].copy_from_slice(chunk);
            self.write32(BUFFER, u32::from_le_bytes(data));
        }
        self.wait_status(INT_TRANSFER_COMPLETE, deadline_us)?;
        Ok(())
    }
}

fn reached(now: u64, deadline: u64) -> bool {
    now.wrapping_sub(deadline) as i64 >= 0
}

fn transfer_mode(direction: DataDirection) -> u16 {
    match direction {
        DataDirection::Read => (1 << 1) | (1 << 4),
        DataDirection::Write => 1 << 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> u64 {
        static TIME: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
        TIME.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
    }

    #[test]
    fn bcm2835_subword_access_preserves_adjacent_fields() {
        let mut registers = [0u32; 64];
        registers[HOST_CONTROL / 4] = 0x1234_5678;
        let mut host =
            unsafe { Sdhci::new_bcm2835(registers.as_mut_ptr() as usize, 50_000_000, now) };
        host.card_clock_hz = 25_000_000;
        assert_eq!(host.read8(POWER_CONTROL), 0x56);
        host.write8(POWER_CONTROL, 0x0f);
        assert_eq!(registers[HOST_CONTROL / 4], 0x1234_0f78);
        host.write16(HOST_CONTROL + 2, 0xabcd);
        assert_eq!(registers[HOST_CONTROL / 4], 0xabcd_0f78);
        assert_eq!(host.read16(HOST_CONTROL + 2), 0xabcd);
    }

    #[test]
    fn bcm2835_preparation_does_not_reissue_previous_command() {
        let mut registers = [0u32; 64];
        registers[TRANSFER_MODE / 4] = 0x081a_0000; // previous CMD8
        let mut host =
            unsafe { Sdhci::new_bcm2835(registers.as_mut_ptr() as usize, 50_000_000, now) };
        host.card_clock_hz = 25_000_000;
        host.prepare_data(DataDirection::Read, 512, 1).unwrap();
        assert_eq!(registers[BLOCK_SIZE / 4], 0x0001_0200);
        assert_eq!(registers[TRANSFER_MODE / 4], 0x081a_0000);
        assert_eq!(host.prepared, Some((DataDirection::Read, 512, 1)));
    }
}

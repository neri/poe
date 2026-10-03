//! Polled PIO driver for the Synopsys DesignWare Mobile Storage Host.
//!
//! This driver intentionally does not enable IDMAC. It transfers data through
//! the controller FIFO and relies only on the register interface shared by
//! the JH7110 DW-MMC implementation.

use super::{BusWidth, DataDirection, Host, HostError, Response, ResponseType};
use crate::System;

const CTRL: usize = 0x000;
const PWREN: usize = 0x004;
const CLKDIV: usize = 0x008;
const CLKSRC: usize = 0x00c;
const CLKENA: usize = 0x010;
const TMOUT: usize = 0x014;
const CTYPE: usize = 0x018;
const BLKSIZ: usize = 0x01c;
const BYTCNT: usize = 0x020;
const CMDARG: usize = 0x028;
const CMD: usize = 0x02c;
const RESP0: usize = 0x030;
const RINTSTS: usize = 0x044;
const STATUS: usize = 0x048;
const FIFOTH: usize = 0x04c;
const TCBCNT: usize = 0x05c;
const TBBCNT: usize = 0x060;
const VERID: usize = 0x06c;
const HCON: usize = 0x070;
const DATA_LEGACY: usize = 0x100;
const DATA_240A: usize = 0x200;

const CTRL_RESET: u32 = 1 << 0;
const CTRL_FIFO_RESET: u32 = 1 << 1;
const CTRL_DMA_RESET: u32 = 1 << 2;
const CLK_ENABLE: u32 = 1;
const CMD_START: u32 = 1 << 31;
const CMD_UPDATE_CLOCK: u32 = 1 << 21;
const CMD_STOP_ABORT: u32 = 1 << 14;
const CMD_WAIT_PREV_DATA: u32 = 1 << 13;
const CMD_SEND_AUTO_STOP: u32 = 1 << 12;
const CMD_DATA_WRITE: u32 = 1 << 10;
const CMD_DATA_EXPECTED: u32 = 1 << 9;
const CMD_RESP_CRC: u32 = 1 << 8;
const CMD_RESP_LONG: u32 = 1 << 7;
const CMD_RESP_EXPECTED: u32 = 1 << 6;

const INT_HARDWARE_LOCK: u32 = 1 << 12;
const INT_DATA_TIMEOUT: u32 = 1 << 9;
const INT_RESP_TIMEOUT: u32 = 1 << 8;
const INT_DATA_CRC: u32 = 1 << 7;
const INT_RESP_CRC: u32 = 1 << 6;
const INT_DATA_OVER: u32 = 1 << 3;
const INT_CMD_DONE: u32 = 1 << 2;
const INT_RESP_ERROR: u32 = 1 << 1;
// FRUN is reported by some DW-MMC implementations around PIO FIFO service.
// Linux does not include it in its fatal data error mask; completion, timeout,
// and CRC status determine whether the transfer itself failed.
const INT_ERRORS: u32 = INT_HARDWARE_LOCK
    | INT_DATA_TIMEOUT
    | INT_RESP_TIMEOUT
    | INT_DATA_CRC
    | INT_RESP_CRC
    | INT_RESP_ERROR;

const STATUS_FIFO_FULL: u32 = 1 << 3;
const STATUS_FIFO_EMPTY: u32 = 1 << 2;
const STATUS_BUSY: u32 = 1 << 9;
const RESPONSE_TIMEOUT_CLOCKS: u32 = 0xff;
const DATA_TIMEOUT_CLOCKS: u32 = 0x00ff_ffff;

/// One DW-MMC instance. The platform must enable its bus and card clocks and
/// configure the card pins before registering this host.
pub struct DwMmc {
    base: usize,
    input_clock_hz: u32,
    fifo_depth: u16,
    now_us: fn() -> u64,
    data_offset: usize,
    prepared: Option<(DataDirection, u16, u16)>,
}

impl DwMmc {
    /// # Safety
    /// `base` must point to a mapped DW-MMC register bank. The bus clock must
    /// already be enabled and `input_clock_hz` must match the card-interface
    /// clock supplied to the controller.
    pub unsafe fn new(
        base: usize,
        input_clock_hz: u32,
        fifo_depth: u16,
        now_us: fn() -> u64,
    ) -> Self {
        let version = unsafe { ((base + VERID) as *const u32).read_volatile() } & 0xffff;
        Self {
            base,
            input_clock_hz,
            fifo_depth: fifo_depth.max(1),
            now_us,
            data_offset: if version >= 0x240a {
                DATA_240A
            } else {
                DATA_LEGACY
            },
            prepared: None,
        }
    }

    fn read(&self, offset: usize) -> u32 {
        unsafe { ((self.base + offset) as *const u32).read_volatile() }
    }

    fn write(&self, offset: usize, value: u32) {
        unsafe { ((self.base + offset) as *mut u32).write_volatile(value) }
    }

    fn wait_interrupt(&self, mask: u32, deadline: u64) -> Result<u32, HostError> {
        loop {
            let status = self.read(RINTSTS);
            if status & INT_ERRORS != 0 {
                let controller_status = self.read(STATUS);
                crate::println!(
                    "DW-MMC interrupt error: int={:08x} status={:08x} cmd={:08x} verid={:08x} hcon={:08x} fifoth={:08x} bytcnt={:08x} tcbc={:08x} tbb={:08x} data_off={:#x}",
                    status,
                    controller_status,
                    self.read(CMD),
                    self.read(VERID),
                    self.read(HCON),
                    self.read(FIFOTH),
                    self.read(BYTCNT),
                    self.read(TCBCNT),
                    self.read(TBBCNT),
                    self.data_offset
                );
                self.write(RINTSTS, status);
                return Err(if status & (INT_RESP_TIMEOUT | INT_DATA_TIMEOUT) != 0 {
                    HostError::Timeout
                } else if status & (INT_RESP_CRC | INT_DATA_CRC) != 0 {
                    HostError::Crc
                } else {
                    HostError::Io
                });
            }
            if status & mask != 0 {
                self.write(RINTSTS, status & mask);
                return Ok(status);
            }
            if reached((self.now_us)(), deadline) {
                crate::println!(
                    "DW-MMC interrupt timeout: cmd={:08x} int={:08x} status={:08x}",
                    self.read(CMD),
                    self.read(RINTSTS),
                    self.read(STATUS)
                );
                return Err(HostError::Timeout);
            }
            core::hint::spin_loop();
        }
    }

    fn wait_command_idle(&self, deadline: u64) -> Result<(), HostError> {
        loop {
            if self.read(CMD) & CMD_START == 0 {
                return Ok(());
            }
            if reached((self.now_us)(), deadline) {
                crate::println!(
                    "DW-MMC command busy timeout: cmd={:08x} int={:08x} status={:08x}",
                    self.read(CMD),
                    self.read(RINTSTS),
                    self.read(STATUS)
                );
                return Err(HostError::Timeout);
            }
            core::hint::spin_loop();
        }
    }

    fn update_clock(&self, deadline: u64) -> Result<(), HostError> {
        self.write(CMDARG, 0);
        self.write(CMD, CMD_START | CMD_UPDATE_CLOCK | CMD_WAIT_PREV_DATA);
        self.wait_command_idle(deadline)?;
        let status = self.read(RINTSTS);
        self.write(RINTSTS, status);
        if status & INT_ERRORS != 0 {
            return Err(HostError::Io);
        }
        Ok(())
    }

    fn response(&self, kind: ResponseType) -> Response {
        match kind {
            ResponseType::None => Response::default(),
            ResponseType::R2 => Response {
                short: 0,
                // RESP0 contains the least-significant word in the DW-MMC
                // response register bank. The host contract uses network order.
                long: [
                    self.read(RESP0 + 12),
                    self.read(RESP0 + 8),
                    self.read(RESP0 + 4),
                    self.read(RESP0),
                ],
            },
            _ => Response {
                short: self.read(RESP0),
                long: [0; 4],
            },
        }
    }

    fn transfer(
        &self,
        bytes: &mut [u8],
        direction: DataDirection,
        deadline: u64,
    ) -> Result<(), HostError> {
        let mut offset = 0;
        while offset < bytes.len() {
            match direction {
                DataDirection::Read => {
                    if self.read(STATUS) & STATUS_FIFO_EMPTY == 0 {
                        let word = self.read(self.data_offset).to_le_bytes();
                        let count = (bytes.len() - offset).min(4);
                        bytes[offset..offset + count].copy_from_slice(&word[..count]);
                        offset += count;
                    } else {
                        let status = self.read(RINTSTS);
                        if status & INT_ERRORS != 0 {
                            self.write(RINTSTS, status);
                            return Err(if status & (INT_DATA_TIMEOUT | INT_RESP_TIMEOUT) != 0 {
                                HostError::Timeout
                            } else if status & (INT_DATA_CRC | INT_RESP_CRC) != 0 {
                                HostError::Crc
                            } else {
                                HostError::Io
                            });
                        }
                        if reached((self.now_us)(), deadline) {
                            return Err(HostError::Timeout);
                        }
                        core::hint::spin_loop();
                    }
                }
                DataDirection::Write => {
                    if self.read(STATUS) & STATUS_FIFO_FULL == 0 {
                        let count = (bytes.len() - offset).min(4);
                        let mut word = [0; 4];
                        word[..count].copy_from_slice(&bytes[offset..offset + count]);
                        self.write(self.data_offset, u32::from_le_bytes(word));
                        offset += count;
                    } else {
                        let status = self.read(RINTSTS);
                        if status & INT_ERRORS != 0 {
                            self.write(RINTSTS, status);
                            return Err(if status & (INT_DATA_TIMEOUT | INT_RESP_TIMEOUT) != 0 {
                                HostError::Timeout
                            } else if status & (INT_DATA_CRC | INT_RESP_CRC) != 0 {
                                HostError::Crc
                            } else {
                                HostError::Io
                            });
                        }
                        if reached((self.now_us)(), deadline) {
                            return Err(HostError::Timeout);
                        }
                        core::hint::spin_loop();
                    }
                }
            }
        }
        self.wait_interrupt(INT_DATA_OVER, deadline)
            .map_err(|error| {
                crate::println!(
                    "DW-MMC data transfer failed: direction={:?} bytes={} fifo_words={}",
                    direction,
                    bytes.len(),
                    (self.read(STATUS) >> 17) & 0x1fff
                );
                error
            })?;
        if direction == DataDirection::Write {
            loop {
                if self.read(STATUS) & STATUS_BUSY == 0 {
                    return Ok(());
                }
                if reached((self.now_us)(), deadline) {
                    return Err(HostError::Timeout);
                }
                core::hint::spin_loop();
            }
        }
        Ok(())
    }
}

impl Host for DwMmc {
    fn now_us(&self) -> u64 {
        (self.now_us)()
    }

    fn reset(&mut self) -> Result<(), HostError> {
        self.write(CTRL, CTRL_RESET | CTRL_FIFO_RESET | CTRL_DMA_RESET);
        let deadline = self.now_us().wrapping_add(100_000);
        loop {
            if self.read(CTRL) & (CTRL_RESET | CTRL_FIFO_RESET | CTRL_DMA_RESET) == 0 {
                break;
            }
            if reached(self.now_us(), deadline) {
                return Err(HostError::Timeout);
            }
            core::hint::spin_loop();
        }
        self.write(PWREN, 1);
        self.write(TMOUT, (DATA_TIMEOUT_CLOCKS << 8) | RESPONSE_TIMEOUT_CLOCKS);
        // Keep the FIFO thresholds valid for PIO as well as DMA. In
        // particular, RX_WMARK is depth/2 - 1 and TX_WMARK is depth/2; a
        // zero TX watermark can cause FIFO run errors on some DW-MMC hosts.
        let half_depth = self.fifo_depth as u32 / 2;
        let rx_watermark = half_depth.saturating_sub(1).min(0x0fff);
        let tx_watermark = half_depth.min(0x0fff);
        self.write(
            FIFOTH,
            (2 << 28) | (rx_watermark << 16) | tx_watermark,
        );
        self.write(RINTSTS, u32::MAX);
        self.prepared = None;
        Ok(())
    }

    fn set_clock(&mut self, hz: u32) -> Result<u32, HostError> {
        if hz == 0 || self.input_clock_hz == 0 {
            return Err(HostError::Unsupported);
        }
        self.write(CLKENA, 0);
        self.write(CLKSRC, 0);
        self.update_clock(self.now_us().wrapping_add(100_000))?;
        let divisor = self.input_clock_hz.div_ceil(hz.saturating_mul(2)).max(1);
        if divisor > 255 {
            return Err(HostError::Unsupported);
        }
        self.write(CLKDIV, divisor);
        self.update_clock(self.now_us().wrapping_add(100_000))?;
        self.write(CLKENA, CLK_ENABLE);
        self.update_clock(self.now_us().wrapping_add(100_000))?;
        Ok(self.input_clock_hz / (2 * divisor))
    }

    fn set_bus_width(&mut self, width: BusWidth) -> Result<(), HostError> {
        self.write(CTYPE, if width == BusWidth::Four { 1 } else { 0 });
        Ok(())
    }

    fn prepare_data(
        &mut self,
        direction: DataDirection,
        block_size: u16,
        blocks: u16,
    ) -> Result<(), HostError> {
        let Some(byte_count) = (block_size as u32).checked_mul(blocks as u32) else {
            return Err(HostError::Unsupported);
        };
        if block_size == 0 || blocks != 1 {
            return Err(HostError::Unsupported);
        }
        self.write(BLKSIZ, block_size as u32);
        self.write(BYTCNT, byte_count);
        self.prepared = Some((direction, block_size, blocks));
        Ok(())
    }

    fn command(
        &mut self,
        index: u8,
        argument: u32,
        response: ResponseType,
        deadline: u64,
    ) -> Result<Response, HostError> {
        self.wait_command_idle(deadline).map_err(|error| {
            crate::println!(
                "DW-MMC CMD{}: controller busy before command: {:?}",
                index,
                error
            );
            error
        })?;
        let data = self.prepared;
        // The DW-MMC command index field is six bits wide. SD commands 32–63
        // (notably CMD55 during card initialization) must retain bit 5.
        let mut command = (index as u32) & 0x3f;
        if response != ResponseType::None {
            command |= CMD_RESP_EXPECTED;
            if response != ResponseType::R3 && response != ResponseType::R7 {
                command |= CMD_RESP_CRC;
            }
        }
        if response == ResponseType::R2 {
            command |= CMD_RESP_LONG;
        }
        if index == 0 {
            command |= CMD_STOP_ABORT;
        } else {
            command |= CMD_WAIT_PREV_DATA;
        }
        if let Some((direction, _, blocks)) = data {
            command |= CMD_DATA_EXPECTED;
            if direction == DataDirection::Write {
                command |= CMD_DATA_WRITE;
            }
            if blocks > 1 {
                command |= CMD_SEND_AUTO_STOP;
            }
        }
        self.write(RINTSTS, u32::MAX);
        self.write(CMDARG, argument);
        self.write(CMD, CMD_START | command);
        self.wait_command_idle(deadline).map_err(|error| {
            crate::println!("DW-MMC CMD{}: command did not complete: {:?}", index, error);
            error
        })?;
        if response != ResponseType::None {
            self.wait_interrupt(INT_CMD_DONE, deadline)
                .map_err(|error| {
                    crate::println!("DW-MMC CMD{}: response wait failed: {:?}", index, error);
                    error
                })?;
        }
        if response == ResponseType::R1b {
            loop {
                if self.read(STATUS) & STATUS_BUSY == 0 {
                    break;
                }
                if reached((self.now_us)(), deadline) {
                    return Err(HostError::Timeout);
                }
                core::hint::spin_loop();
            }
        }
        let result = self.response(response);
        Ok(result)
    }

    fn read_data(&mut self, bytes: &mut [u8], deadline_us: u64) -> Result<(), HostError> {
        if !matches!(self.prepared, Some((DataDirection::Read, _, _))) {
            return Err(HostError::Io);
        }
        let result = self.transfer(bytes, DataDirection::Read, deadline_us);
        self.prepared = None;
        result
    }

    fn write_data(&mut self, bytes: &[u8], deadline_us: u64) -> Result<(), HostError> {
        if !matches!(self.prepared, Some((DataDirection::Write, _, _))) {
            return Err(HostError::Io);
        }
        let mut copy = [0u8; 512];
        if bytes.len() > copy.len() {
            return Err(HostError::Unsupported);
        }
        copy[..bytes.len()].copy_from_slice(bytes);
        let result = self.transfer(&mut copy[..bytes.len()], DataDirection::Write, deadline_us);
        self.prepared = None;
        result
    }
}

fn reached(now: u64, deadline: u64) -> bool {
    now.wrapping_sub(deadline) < (1u64 << 63)
}

//! Native SD memory-card protocol, independent of a particular host controller.
//!
//! Hosts provide normalized command responses and synchronous PIO data transfers.
//! This crate deliberately has no allocator or MiniOS dependency.
#![no_std]

use core::ops::Range;

pub const BLOCK_SIZE: usize = 512;
const OCR_READY: u32 = 1 << 31;
const OCR_CCS: u32 = 1 << 30;
const OCR_VOLTAGE: u32 = 0x00ff_8000;
const R1_ERROR_MASK: u32 = 0xfdff_e008;
const ACMD41_POLL_INTERVAL_US: u64 = 10_000;
const ACMD41_TIMEOUT_US: u64 = 2_000_000;
const COMMAND_TIMEOUT_US: u64 = 100_000;
const DATA_TIMEOUT_US: u64 = 250_000;
const BUSY_TIMEOUT_US: u64 = 1_000_000;

/// Response format requested from a host. Hosts validate command and data CRCs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseType {
    None,
    R1,
    R1b,
    R2,
    R3,
    R6,
    R7,
}

/// A normalized SD response. `long` contains the 128 payload bits of an R2
/// response in network order; the host removes the framing and CRC bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Response {
    pub short: u32,
    pub long: [u32; 4],
}

/// Direction and transfer shape for one native SD data command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataDirection {
    Read,
    Write,
}

/// The minimum synchronous host contract required by the card protocol.
///
/// `set_clock` must never exceed the requested frequency. Command and data
/// methods must stop by their absolute microsecond deadline, including when
/// hardware is not responding. `read_data`/`write_data` transfer exactly the
/// provided byte count using the block length previously configured.
pub trait Host {
    fn now_us(&self) -> u64;
    fn reset(&mut self) -> Result<(), HostError>;
    fn set_clock(&mut self, hz: u32) -> Result<u32, HostError>;
    fn set_bus_width(&mut self, width: BusWidth) -> Result<(), HostError>;
    fn prepare_data(
        &mut self,
        direction: DataDirection,
        block_size: u16,
        blocks: u16,
    ) -> Result<(), HostError>;
    fn command(
        &mut self,
        index: u8,
        argument: u32,
        response: ResponseType,
        deadline_us: u64,
    ) -> Result<Response, HostError>;
    fn read_data(&mut self, bytes: &mut [u8], deadline_us: u64) -> Result<(), HostError>;
    fn write_data(&mut self, bytes: &[u8], deadline_us: u64) -> Result<(), HostError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusWidth {
    One,
    Four,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostError {
    Timeout,
    CommandInhibitTimeout,
    CommandCompleteTimeout,
    Crc,
    Removed,
    Busy,
    Unsupported,
    Io,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardError {
    Host(HostError),
    InitializationHostError {
        stage: InitializationStage,
        error: HostError,
    },
    CardStatus(u32),
    Protocol,
    UnsupportedCard,
    Timeout,
    OutOfRange,
    InvalidBuffer,
    NeedsReinitialization,
}

/// Initialization operation that timed out in the host controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitializationStage {
    HostReset,
    InitialBusWidth,
    InitialClock,
    Cmd0,
    Cmd8,
    ApplicationCondition,
    ReadCid,
    ReadRca,
    ReadCsd,
    SelectCard,
    SetBlockLength,
    ReadScr,
    FourBitBusWidth,
    TransferClock,
}

/// Failure details for a multi-block read or write request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferError {
    /// Error reported by the card protocol or host controller.
    pub error: CardError,
    /// First LBA that did not complete. For a request rejected before I/O,
    /// this is the requested starting LBA.
    pub failed_lba: u64,
    /// Number of complete logical blocks transferred before the failure.
    pub completed_blocks: u64,
}

impl From<HostError> for CardError {
    fn from(value: HostError) -> Self {
        Self::Host(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Addressing {
    Byte,
    Block,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardKind {
    Sdsc,
    Sdhc,
    Sdxc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardInfo {
    pub kind: CardKind,
    pub addressing: Addressing,
    pub capacity_blocks: u64,
    pub rca: u16,
    pub bus_width: BusWidth,
    pub clock_hz: u32,
    pub cid: [u32; 4],
    pub csd: [u32; 4],
}

/// Initialized card and its owning host. Operations are synchronous and
/// serialized by ownership of this value.
pub struct Card<H> {
    host: H,
    info: CardInfo,
    usable: bool,
}

impl<H: Host> Card<H> {
    /// Reset the host and initialize an inserted SDSC/SDHC/SDXC memory card.
    pub fn initialize(mut host: H) -> Result<Self, CardError> {
        let info = Self::initialize_inner(&mut host)?;
        Ok(Self {
            host,
            info,
            usable: true,
        })
    }

    /// Reinitialize this card in place, retaining the host if initialization
    /// fails so a later call can retry after the hardware or card recovers.
    pub fn reinitialize(&mut self) -> Result<(), CardError> {
        self.usable = false;
        let info = Self::initialize_inner(&mut self.host)?;
        self.info = info;
        self.usable = true;
        Ok(())
    }

    fn initialize_inner(host: &mut H) -> Result<CardInfo, CardError> {
        let mut stage = InitializationStage::HostReset;
        let result = (|| {
            host.reset()?;
            stage = InitializationStage::InitialBusWidth;
            host.set_bus_width(BusWidth::One)?;
            stage = InitializationStage::InitialClock;
            let init_hz = host.set_clock(400_000)?;
            if init_hz == 0 || init_hz > 400_000 {
                return Err(CardError::Protocol);
            }
            // Keep CMD and DAT high while the initial clock runs. At 400 kHz,
            // one millisecond is well over the required 74 clocks.
            wait_us(host, 1_000);

            stage = InitializationStage::Cmd0;
            host.command(0, 0, ResponseType::None, deadline(host, COMMAND_TIMEOUT_US))?;
            // Give the card time to complete the idle-state transition before
            // probing it. This is especially important on hosts where CMD0
            // has no response interrupt to provide additional settling time.
            wait_us(host, 1_000);
            stage = InitializationStage::Cmd8;
            let cmd8 = host.command(
                8,
                0x1aa,
                ResponseType::R7,
                deadline(host, COMMAND_TIMEOUT_US),
            );
            let v2 = match cmd8 {
                Ok(r) if r.short & 0xfff == 0x1aa => true,
                Ok(r) if r.short & (1 << 22) != 0 => false,
                Err(HostError::Timeout) => false,
                Err(error) => return Err(error.into()),
                _ => return Err(CardError::Protocol),
            };

            let started = host.now_us();
            stage = InitializationStage::ApplicationCondition;
            let ocr = loop {
                let prefix =
                    host.command(55, 0, ResponseType::R1, deadline(host, COMMAND_TIMEOUT_US))?;
                check_r1(prefix.short)?;
                let response = host.command(
                    41,
                    OCR_VOLTAGE | if v2 { 1 << 30 } else { 0 },
                    ResponseType::R3,
                    deadline(host, COMMAND_TIMEOUT_US),
                )?;
                let ocr = response.short;
                if ocr & OCR_READY != 0 {
                    break ocr;
                }
                if host.now_us().wrapping_sub(started) >= ACMD41_TIMEOUT_US {
                    return Err(CardError::Timeout);
                }
                // Some cards need a few milliseconds between operating-
                // condition polls. Busy-polling CMD55/ACMD41 can make card
                // initialization depend on incidental console output.
                wait_us(host, ACMD41_POLL_INTERVAL_US);
            };
            if ocr & OCR_VOLTAGE == 0 {
                return Err(CardError::UnsupportedCard);
            }
            let addressing = if ocr & OCR_CCS != 0 {
                if !v2 {
                    return Err(CardError::UnsupportedCard);
                }
                Addressing::Block
            } else {
                Addressing::Byte
            };

            stage = InitializationStage::ReadCid;
            let cid = host
                .command(2, 0, ResponseType::R2, deadline(host, COMMAND_TIMEOUT_US))?
                .long;
            stage = InitializationStage::ReadRca;
            let r6 = host.command(3, 0, ResponseType::R6, deadline(host, COMMAND_TIMEOUT_US))?;
            check_r6(r6.short)?;
            let rca = (r6.short >> 16) as u16;
            if rca == 0 {
                return Err(CardError::Protocol);
            }
            stage = InitializationStage::ReadCsd;
            let csd = host
                .command(
                    9,
                    (rca as u32) << 16,
                    ResponseType::R2,
                    deadline(host, COMMAND_TIMEOUT_US),
                )?
                .long;
            let structure = bits128(&csd, 126..128);
            let capacity_blocks = match structure {
                0 if addressing == Addressing::Byte => csd_v1_blocks(&csd)?,
                1 if addressing == Addressing::Block => csd_v2_blocks(&csd)?,
                _ => return Err(CardError::UnsupportedCard),
            };
            let kind = if addressing == Addressing::Byte {
                CardKind::Sdsc
            } else if capacity_blocks <= (32u64 * 1024 * 1024 * 1024 / 512) {
                CardKind::Sdhc
            } else {
                CardKind::Sdxc
            };

            stage = InitializationStage::SelectCard;
            let selected = host.command(
                7,
                (rca as u32) << 16,
                ResponseType::R1b,
                deadline(host, BUSY_TIMEOUT_US),
            )?;
            check_r1(selected.short)?;
            if addressing == Addressing::Byte {
                stage = InitializationStage::SetBlockLength;
                let r = host.command(
                    16,
                    BLOCK_SIZE as u32,
                    ResponseType::R1,
                    deadline(host, COMMAND_TIMEOUT_US),
                )?;
                check_r1(r.short)?;
            }

            stage = InitializationStage::ReadScr;
            let scr = read_scr(host, rca)?;
            // SCR.SD_BUS_WIDTHS occupies bits 51:48 of the 64-bit SCR, hence
            // bits 19:16 of the first (most significant) word. Bit 2 advertises
            // four-bit support.
            let width = if (scr[0] >> 16) & (1 << 2) != 0 {
                stage = InitializationStage::FourBitBusWidth;
                let r = app_command(host, rca, 6, 2)?;
                check_r1(r.short)?;
                host.set_bus_width(BusWidth::Four)?;
                BusWidth::Four
            } else {
                BusWidth::One
            };
            stage = InitializationStage::TransferClock;
            let requested_hz = 25_000_000;
            let clock_hz = host.set_clock(requested_hz)?;
            if clock_hz == 0 || clock_hz > requested_hz {
                return Err(CardError::Protocol);
            }

            Ok(CardInfo {
                kind,
                addressing,
                capacity_blocks,
                rca,
                bus_width: width,
                clock_hz,
                cid,
                csd,
            })
        })();
        result.map_err(|error| match error {
            CardError::Host(error) => CardError::InitializationHostError { stage, error },
            other => other,
        })
    }

    pub const fn info(&self) -> &CardInfo {
        &self.info
    }

    /// Whether I/O may continue without reinitializing the host and card.
    /// A failed data operation can leave the card state ambiguous, so the
    /// caller must call `reinitialize` before issuing more requests.
    pub const fn is_usable(&self) -> bool {
        self.usable
    }

    pub fn into_host(self) -> H {
        self.host
    }

    /// Read whole 512-byte logical blocks. A host/protocol error may leave the
    /// output buffer partially updated.
    pub fn read(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), CardError> {
        self.read_detailed(lba, buffer)
            .map_err(|failure| failure.error)
    }

    /// Read blocks while preserving the failing LBA and partial progress.
    pub fn read_detailed(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), TransferError> {
        self.transfer_detailed(lba, buffer)
    }

    /// Write whole 512-byte logical blocks. Errors are never retried because
    /// the card may have accepted some or all of the data.
    pub fn write(&mut self, lba: u64, buffer: &[u8]) -> Result<(), CardError> {
        self.write_detailed(lba, buffer)
            .map_err(|failure| failure.error)
    }

    /// Write blocks while preserving the failing LBA and partial progress.
    pub fn write_detailed(&mut self, lba: u64, buffer: &[u8]) -> Result<(), TransferError> {
        if !self.usable {
            return Err(transfer_error(lba, 0, CardError::NeedsReinitialization));
        }
        let range = self
            .validate_range(lba, buffer.len())
            .map_err(|error| transfer_error(lba, 0, error))?;
        for (index, chunk) in buffer.chunks_exact(BLOCK_SIZE).enumerate() {
            let block = range.start + index as u64;
            let arg = self
                .command_address(block)
                .map_err(|error| transfer_error(block, index as u64, error))?;
            let result = (|| {
                self.host
                    .prepare_data(DataDirection::Write, BLOCK_SIZE as u16, 1)?;
                let response = self.host.command(
                    24,
                    arg,
                    ResponseType::R1,
                    deadline(&self.host, COMMAND_TIMEOUT_US),
                )?;
                check_r1(response.short)?;
                self.host
                    .write_data(chunk, deadline(&self.host, DATA_TIMEOUT_US))?;
                self.wait_transfer_state()
            })();
            if result.is_err() {
                self.usable = false;
                return result.map_err(|error| transfer_error(block, index as u64, error));
            }
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), CardError> {
        if !self.usable {
            return Err(CardError::NeedsReinitialization);
        }
        self.wait_transfer_state()
    }

    fn transfer_detailed(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), TransferError> {
        if !self.usable {
            return Err(transfer_error(lba, 0, CardError::NeedsReinitialization));
        }
        let range = self
            .validate_range(lba, buffer.len())
            .map_err(|error| transfer_error(lba, 0, error))?;
        for (index, chunk) in buffer.chunks_exact_mut(BLOCK_SIZE).enumerate() {
            let block = range.start + index as u64;
            let arg = self
                .command_address(block)
                .map_err(|error| transfer_error(block, index as u64, error))?;
            let result = (|| {
                self.host
                    .prepare_data(DataDirection::Read, BLOCK_SIZE as u16, 1)?;
                let response = self.host.command(
                    17,
                    arg,
                    ResponseType::R1,
                    deadline(&self.host, COMMAND_TIMEOUT_US),
                )?;
                check_r1(response.short)?;
                self.host
                    .read_data(chunk, deadline(&self.host, DATA_TIMEOUT_US))?;
                Ok(())
            })();
            if result.is_err() {
                self.usable = false;
                return result.map_err(|error| transfer_error(block, index as u64, error));
            }
        }
        Ok(())
    }

    fn validate_range(&self, lba: u64, bytes: usize) -> Result<Range<u64>, CardError> {
        if bytes % BLOCK_SIZE != 0 {
            return Err(CardError::InvalidBuffer);
        }
        let count = (bytes / BLOCK_SIZE) as u64;
        let end = lba.checked_add(count).ok_or(CardError::OutOfRange)?;
        if end > self.info.capacity_blocks || (count == 0 && lba > self.info.capacity_blocks) {
            return Err(CardError::OutOfRange);
        }
        Ok(lba..end)
    }

    fn command_address(&self, lba: u64) -> Result<u32, CardError> {
        let address = match self.info.addressing {
            Addressing::Block => lba,
            Addressing::Byte => lba
                .checked_mul(BLOCK_SIZE as u64)
                .ok_or(CardError::OutOfRange)?,
        };
        u32::try_from(address).map_err(|_| CardError::OutOfRange)
    }

    fn wait_transfer_state(&mut self) -> Result<(), CardError> {
        let until = deadline(&self.host, BUSY_TIMEOUT_US);
        loop {
            let r = self.host.command(
                13,
                (self.info.rca as u32) << 16,
                ResponseType::R1,
                deadline(&self.host, COMMAND_TIMEOUT_US),
            )?;
            check_r1(r.short)?;
            let ready = r.short & (1 << 8) != 0;
            let state = (r.short >> 9) & 0xf;
            if ready && state == 4 {
                return Ok(());
            }
            if reached(self.host.now_us(), until) {
                return Err(CardError::Timeout);
            }
            core::hint::spin_loop();
        }
    }
}

const fn transfer_error(lba: u64, completed_blocks: u64, error: CardError) -> TransferError {
    TransferError {
        error,
        failed_lba: lba,
        completed_blocks,
    }
}

fn read_scr<H: Host>(host: &mut H, rca: u16) -> Result<[u32; 2], CardError> {
    let response = host.command(
        55,
        (rca as u32) << 16,
        ResponseType::R1,
        deadline(host, COMMAND_TIMEOUT_US),
    )?;
    check_r1(response.short)?;
    host.prepare_data(DataDirection::Read, 8, 1)?;
    let response = host.command(51, 0, ResponseType::R1, deadline(host, COMMAND_TIMEOUT_US))?;
    check_r1(response.short)?;
    let mut bytes = [0u8; 8];
    host.read_data(&mut bytes, deadline(host, DATA_TIMEOUT_US))?;
    Ok([
        u32::from_be_bytes(bytes[..4].try_into().unwrap()),
        u32::from_be_bytes(bytes[4..].try_into().unwrap()),
    ])
}

fn app_command<H: Host>(
    host: &mut H,
    rca: u16,
    command: u8,
    arg: u32,
) -> Result<Response, CardError> {
    let prefix = host.command(
        55,
        (rca as u32) << 16,
        ResponseType::R1,
        deadline(host, COMMAND_TIMEOUT_US),
    )?;
    check_r1(prefix.short)?;
    host.command(
        command,
        arg,
        ResponseType::R1,
        deadline(host, COMMAND_TIMEOUT_US),
    )
    .map_err(Into::into)
}

fn check_r1(value: u32) -> Result<(), CardError> {
    let status = value & R1_ERROR_MASK;
    if status != 0 {
        Err(CardError::CardStatus(status))
    } else {
        Ok(())
    }
}

fn check_r6(value: u32) -> Result<(), CardError> {
    let status = value & 0xe000;
    if status != 0 {
        Err(CardError::CardStatus(status))
    } else {
        Ok(())
    }
}

fn deadline<H: Host>(host: &H, delta: u64) -> u64 {
    host.now_us().wrapping_add(delta)
}

fn reached(now: u64, deadline: u64) -> bool {
    now.wrapping_sub(deadline) as i64 >= 0
}

fn wait_us<H: Host>(host: &H, duration: u64) {
    let started = host.now_us();
    while host.now_us().wrapping_sub(started) < duration {
        core::hint::spin_loop();
    }
}

fn bits128(words: &[u32; 4], range: Range<u32>) -> u128 {
    let all = ((words[0] as u128) << 96)
        | ((words[1] as u128) << 64)
        | ((words[2] as u128) << 32)
        | words[3] as u128;
    let width = range.end - range.start;
    (all >> range.start) & ((1u128 << width) - 1)
}

fn csd_v1_blocks(csd: &[u32; 4]) -> Result<u64, CardError> {
    let read_bl_len = bits128(csd, 80..84) as u32;
    let c_size = bits128(csd, 62..74) as u64;
    let c_size_mult = bits128(csd, 47..50) as u32;
    if read_bl_len > 11 {
        return Err(CardError::UnsupportedCard);
    }
    let bytes = (c_size + 1)
        .checked_mul(1u64 << (c_size_mult + 2))
        .and_then(|v| v.checked_mul(1u64 << read_bl_len))
        .ok_or(CardError::Protocol)?;
    if bytes % BLOCK_SIZE as u64 != 0 {
        return Err(CardError::Protocol);
    }
    Ok(bytes / BLOCK_SIZE as u64)
}

fn csd_v2_blocks(csd: &[u32; 4]) -> Result<u64, CardError> {
    let c_size = bits128(csd, 48..70) as u64;
    c_size
        .checked_add(1)
        .and_then(|v| v.checked_mul(1024))
        .ok_or(CardError::Protocol)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(value: u128) -> [u32; 4] {
        [
            (value >> 96) as u32,
            (value >> 64) as u32,
            (value >> 32) as u32,
            value as u32,
        ]
    }

    #[test]
    fn sdsc_capacity_uses_all_size_and_multiplier_bits() {
        // Exercise each multiplier, including bit 49, and C_SIZE bit 73.
        for size in [0u128, 127, 2047, 2048, 4095] {
            for multiplier in 0u128..8 {
                let csd = words((9 << 80) | (size << 62) | (multiplier << 47));
                assert_eq!(
                    csd_v1_blocks(&csd).unwrap(),
                    ((size + 1) << (multiplier + 2)) as u64
                );
            }
        }
    }

    #[test]
    fn sdsc_64_mib_and_sdhc_4_gib_capacity() {
        let csd = words((9 << 80) | (255 << 62) | (7 << 47));
        assert_eq!(csd_v1_blocks(&csd).unwrap(), 64 * 1024 * 1024 / 512);
        let csd = words((1 << 126) | (8191 << 48));
        assert_eq!(csd_v2_blocks(&csd).unwrap(), 4 * 1024 * 1024 * 1024 / 512);
    }
    #[derive(Default)]
    struct MockHost {
        commands: usize,
        writes: usize,
        fail_write: Option<usize>,
        write_protected: bool,
        busy: bool,
        time: core::cell::Cell<u64>,
        last_argument: u32,
    }

    impl Host for MockHost {
        fn now_us(&self) -> u64 {
            let now = self.time.get();
            self.time.set(now + 100_000);
            now
        }
        fn reset(&mut self) -> Result<(), HostError> {
            Ok(())
        }
        fn set_clock(&mut self, hz: u32) -> Result<u32, HostError> {
            Ok(hz)
        }
        fn set_bus_width(&mut self, _: BusWidth) -> Result<(), HostError> {
            Ok(())
        }
        fn prepare_data(&mut self, _: DataDirection, _: u16, _: u16) -> Result<(), HostError> {
            Ok(())
        }
        fn command(
            &mut self,
            index: u8,
            argument: u32,
            _: ResponseType,
            _: u64,
        ) -> Result<Response, HostError> {
            self.commands += 1;
            self.last_argument = argument;
            let short = if index == 24 && self.write_protected {
                1 << 26 // WP_VIOLATION
            } else if index == 13 && !self.busy {
                (1 << 8) | (4 << 9) // READY_FOR_DATA, TRAN
            } else {
                0
            };
            Ok(Response {
                short,
                ..Response::default()
            })
        }
        fn read_data(&mut self, bytes: &mut [u8], _: u64) -> Result<(), HostError> {
            bytes.fill(0x5a);
            Ok(())
        }
        fn write_data(&mut self, _: &[u8], _: u64) -> Result<(), HostError> {
            self.writes += 1;
            if self.fail_write == Some(self.writes) {
                Err(HostError::Crc)
            } else {
                Ok(())
            }
        }
    }

    fn card(host: MockHost, addressing: Addressing) -> Card<MockHost> {
        Card {
            host,
            info: CardInfo {
                kind: CardKind::Sdhc,
                addressing,
                capacity_blocks: 131072,
                rca: 1,
                bus_width: BusWidth::One,
                clock_hz: 25_000_000,
                cid: [0; 4],
                csd: [0; 4],
            },
            usable: true,
        }
    }

    #[test]
    fn write_protection_rejects_data_and_requires_reinitialization() {
        let mut card = card(
            MockHost {
                write_protected: true,
                ..MockHost::default()
            },
            Addressing::Block,
        );
        assert_eq!(
            card.write(7, &[0; 512]),
            Err(CardError::CardStatus(1 << 26))
        );
        assert_eq!(card.host.writes, 0);
        let commands = card.host.commands;
        assert_eq!(
            card.write(7, &[0; 512]),
            Err(CardError::NeedsReinitialization)
        );
        assert_eq!(card.host.commands, commands);
    }

    #[test]
    fn partial_write_reports_progress_without_retry() {
        let mut card = card(
            MockHost {
                fail_write: Some(2),
                ..MockHost::default()
            },
            Addressing::Block,
        );
        assert_eq!(
            card.write_detailed(10, &[0; 1536]),
            Err(TransferError {
                error: CardError::Host(HostError::Crc),
                failed_lba: 11,
                completed_blocks: 1,
            })
        );
        assert_eq!(card.host.writes, 2);
        assert!(!card.is_usable());
    }

    #[test]
    fn busy_timeout_does_not_retry_the_write() {
        let mut card = card(
            MockHost {
                busy: true,
                ..MockHost::default()
            },
            Addressing::Block,
        );
        assert_eq!(card.write(0, &[0; 512]), Err(CardError::Timeout));
        assert_eq!(card.host.writes, 1);
        assert!(!card.is_usable());
    }

    #[test]
    fn invalid_ranges_do_not_access_the_host() {
        let mut card = card(MockHost::default(), Addressing::Block);
        for (lba, bytes, error) in [
            (0, 1, CardError::InvalidBuffer),
            (131072, 512, CardError::OutOfRange),
            (u64::MAX, 512, CardError::OutOfRange),
        ] {
            assert_eq!(card.read(lba, &mut [0; 512][..bytes]), Err(error));
        }
        assert_eq!(card.host.commands, 0);
        assert!(card.is_usable());
    }

    #[test]
    fn sdsc_commands_use_byte_addresses_and_sdhc_uses_blocks() {
        for (addressing, argument) in [(Addressing::Byte, 7 * 512), (Addressing::Block, 7)] {
            let mut card = card(MockHost::default(), addressing);
            card.read(7, &mut [0; 512]).unwrap();
            assert_eq!(card.host.last_argument, argument);
        }
    }
}

//! The Bulk-Only Transport state machine, independent of any controller.
//!
//! [`Bot`] never touches hardware.  It says which transfer it wants next
//! ([`Bot::wanted`]); a backend performs that transfer however its controller
//! does it and hands the outcome back ([`Bot::complete`]).  The same state
//! machine therefore runs on the DWC2 of a Raspberry Pi 3, on xHCI, and
//! against the fake device the tests use.
//!
//! What a backend promises in return (see `docs/USB_MSC_RPI_PLAN.md`, 4):
//!
//! * At most one transfer per interface is outstanding, and every transfer it
//!   starts is completed exactly once — with an error at the latest by the
//!   deadline [`Bot::start`] returned.  A completion is only reported once the
//!   controller has stopped using the memory, so a buffer is never reused
//!   under a live DMA.
//! * A bulk transfer asks for at most `max_chunk` bytes, a multiple of the
//!   endpoint's packet size, and reports how many bytes actually moved.  A
//!   NAK is retried inside the backend and never reaches this layer.
//! * `ClearHalt` resets the host's view of the endpoint (data toggle, and on
//!   xHCI the endpoint context) as well as sending CLEAR_FEATURE, and succeeds
//!   only once both are done.
//!
//! The data stage is one stage however many transfers the backend needs for
//! it: its length is tracked here, and a short transfer ends it.

use alloc::vec::Vec;

use libusb::{SetupPacket, UsbError};

use super::scsi::Cdb;
use super::wire::{self, CBW_LEN, CSW_LEN, Cbw, Csw, CswStatus};

/// From the CBW leaving to the CSW arriving.  Progress does not extend it.
pub const COMMAND_TIMEOUT_US: u64 = 5_000_000;
/// One control request: GET_MAX_LUN, Mass Storage Reset, CLEAR_FEATURE.
pub const CONTROL_TIMEOUT_US: u64 = 1_000_000;
/// Between Mass Storage Reset and the first CLEAR_FEATURE.  U-Boot waits this
/// long, and some Full Speed bridges do not answer an immediate one.
pub const RESET_SETTLE_US: u64 = 150_000;
/// What a whole Reset Recovery can take: three control requests and the
/// settle time.
pub const RECOVERY_BUDGET_US: u64 = 3 * CONTROL_TIMEOUT_US + RESET_SETTLE_US;
/// Largest data stage of one command.  Each command costs a CBW and a CSW
/// whatever its size, so this is kept large: 64 KiB is also what Linux's
/// usb-storage asks for per command by default.
pub const MAX_COMMAND_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pipe {
    In,
    Out,
}

/// A transfer the state machine wants performed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Transfer {
    /// Send [`Bot::out_data`] on the bulk OUT endpoint.
    BulkOut { len: usize },
    /// Receive up to `len` bytes on the bulk IN endpoint into
    /// [`Bot::in_buffer`].
    BulkIn { len: usize },
    /// A request on endpoint zero.  Data, if the request has an IN data
    /// stage, goes into [`Bot::in_buffer`].
    Control { setup: SetupPacket },
    /// Clear a halt on one bulk endpoint, on both the device and the host.
    ClearHalt { pipe: Pipe },
}

/// Why a command did not produce a status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BotError {
    Disconnected,
    /// The command deadline passed.  Reset Recovery has run.
    Timeout,
    /// A transfer failed in a way BOT has no finer answer to.  Reset Recovery
    /// has run.
    Transport(UsbError),
    PhaseError,
    /// The CSW was not valid or not meaningful (section 6.3).
    InvalidCsw,
    /// Reset Recovery itself failed.  The interface is not usable until the
    /// device is enumerated again.
    RecoveryFailed(UsbError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BotResult {
    /// A CSW arrived and matched the command.  `transferred` is what the
    /// host actually received; `residue` is what the device says it did not
    /// send.  Neither is judged here.
    Command {
        status: CswStatus,
        residue: u32,
        transferred: usize,
    },
    /// The result of [`Bot::control`].
    Control(Result<usize, UsbError>),
    Failed(BotError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Idle,
    Control,
    Cbw,
    DataIn,
    /// The data stage stalled: clear the IN halt, then read the CSW.
    ClearDataIn,
    Csw,
    /// The CSW stalled once: clear the IN halt and try once more.
    ClearCsw,
    ResetDevice,
    ResetSettle {
        until: u64,
    },
    ResetClearIn,
    ResetClearOut,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BotStats {
    pub commands: u64,
    pub transfers: u64,
    /// Bytes the controller reported as received in data IN stages.
    pub bytes_in: u64,
    pub stalls: u64,
    pub timeouts: u64,
    pub recoveries: u64,
    pub recovery_failures: u64,
    pub invalid_csws: u64,
    pub phase_errors: u64,
}

pub struct Bot {
    interface: u8,
    lun: u8,
    max_chunk: usize,
    tag: u32,
    phase: Phase,
    in_flight: bool,
    cbw: [u8; CBW_LEN],
    csw: [u8; CSW_LEN],
    control_setup: SetupPacket,
    control_data: [u8; 64],
    data: Vec<u8>,
    expected: usize,
    transferred: usize,
    chunk: usize,
    deadline: u64,
    csw_retried: bool,
    /// What went wrong, reported once Reset Recovery has finished.
    failure: Option<BotError>,
    result: Option<BotResult>,
    stats: BotStats,
}

impl Bot {
    /// `max_chunk` is the most one bulk transfer may ask for.  It has to be a
    /// multiple of both endpoints' packet size, so only the final transfer of
    /// a data stage can be short.
    pub fn new(interface: u8, max_chunk: usize) -> Self {
        Self {
            interface,
            lun: 0,
            max_chunk: max_chunk.clamp(1, MAX_COMMAND_BYTES),
            tag: 0,
            phase: Phase::Idle,
            in_flight: false,
            cbw: [0; CBW_LEN],
            csw: [0; CSW_LEN],
            control_setup: SetupPacket::new(0, 0, 0, 0, 0),
            control_data: [0; 64],
            data: Vec::new(),
            expected: 0,
            transferred: 0,
            chunk: 0,
            deadline: 0,
            csw_retried: false,
            failure: None,
            result: None,
            stats: BotStats::default(),
        }
    }

    pub const fn stats(&self) -> BotStats {
        self.stats
    }

    /// True from a command or control request being issued until its result
    /// has been taken.
    pub fn busy(&self) -> bool {
        self.phase != Phase::Idle || self.result.is_some()
    }

    pub fn in_flight(&self) -> bool {
        self.in_flight
    }

    /// Issues a SCSI command with an IN data stage of `data_len` bytes (zero
    /// for none), to finish by `deadline`.
    pub fn command(&mut self, cdb: Cdb, data_len: usize, deadline: u64) -> Result<(), UsbError> {
        if self.busy() || data_len > MAX_COMMAND_BYTES {
            return Err(UsbError::InvalidRequest);
        }
        self.tag = self.tag.wrapping_add(1);
        self.cbw = Cbw {
            tag: self.tag,
            data_length: data_len as u32,
            data_in: true,
            lun: self.lun,
            cdb,
        }
        .to_bytes();
        self.data.clear();
        self.data.resize(data_len, 0);
        self.expected = data_len;
        self.transferred = 0;
        self.deadline = deadline;
        self.csw_retried = false;
        self.failure = None;
        self.phase = Phase::Cbw;
        self.stats.commands += 1;
        Ok(())
    }

    /// Issues a request on endpoint zero outside any command.
    pub fn control(&mut self, setup: SetupPacket) -> Result<(), UsbError> {
        if self.busy() || setup.length as usize > self.control_data.len() {
            return Err(UsbError::InvalidRequest);
        }
        self.control_setup = setup;
        self.phase = Phase::Control;
        Ok(())
    }

    /// The transfer to perform next, if one is due and none is in flight.
    pub fn wanted(&self) -> Option<Transfer> {
        if self.in_flight {
            return None;
        }
        Some(match self.phase {
            Phase::Idle | Phase::ResetSettle { .. } => return None,
            Phase::Control => Transfer::Control {
                setup: self.control_setup,
            },
            Phase::Cbw => Transfer::BulkOut { len: CBW_LEN },
            Phase::DataIn => Transfer::BulkIn {
                len: (self.expected - self.transferred).min(self.max_chunk),
            },
            Phase::Csw => Transfer::BulkIn { len: CSW_LEN },
            Phase::ClearDataIn | Phase::ClearCsw | Phase::ResetClearIn => {
                Transfer::ClearHalt { pipe: Pipe::In }
            }
            Phase::ResetClearOut => Transfer::ClearHalt { pipe: Pipe::Out },
            Phase::ResetDevice => Transfer::Control {
                setup: wire::mass_storage_reset(self.interface),
            },
        })
    }

    /// Marks [`Self::wanted`] as started and returns the deadline the backend
    /// must complete it by.
    pub fn start(&mut self, now: u64) -> u64 {
        self.in_flight = true;
        self.stats.transfers += 1;
        match self.phase {
            Phase::DataIn => {
                self.chunk = (self.expected - self.transferred).min(self.max_chunk);
                self.deadline
            }
            Phase::Cbw | Phase::Csw => self.deadline,
            _ => now.saturating_add(CONTROL_TIMEOUT_US),
        }
    }

    /// What a bulk OUT transfer sends.
    pub fn out_data(&self) -> &[u8] {
        match self.phase {
            Phase::Cbw => &self.cbw,
            _ => &[],
        }
    }

    /// Where an IN transfer's bytes go.  Sized to the transfer in flight.
    pub fn in_buffer(&mut self) -> &mut [u8] {
        match self.phase {
            Phase::DataIn => &mut self.data[self.transferred..self.transferred + self.chunk],
            Phase::Csw => &mut self.csw,
            Phase::Control => &mut self.control_data[..self.control_setup.length as usize],
            _ => &mut [],
        }
    }

    /// The data stage received so far.
    pub fn data(&self) -> &[u8] {
        &self.data[..self.transferred]
    }

    /// The IN data of the last control request.
    pub fn control_data(&self) -> &[u8] {
        &self.control_data
    }

    pub fn take_result(&mut self) -> Option<BotResult> {
        self.result.take()
    }

    /// Advances time-driven phases.  A bulk phase that a backend could not
    /// even start before the deadline times out the same way a started one
    /// would.
    pub fn poll(&mut self, now: u64) {
        if self.in_flight {
            return;
        }
        match self.phase {
            Phase::ResetSettle { until } if now >= until => self.phase = Phase::ResetClearIn,
            Phase::Cbw | Phase::DataIn | Phase::Csw if now >= self.deadline => {
                self.stats.timeouts += 1;
                self.recover(BotError::Timeout);
            }
            _ => {}
        }
    }

    /// The device has gone: whatever was going on ends now, with no recovery.
    /// The backend has already stopped the transfer in flight, if any.
    pub fn abort(&mut self) {
        let was_busy = self.phase != Phase::Idle;
        self.in_flight = false;
        self.phase = Phase::Idle;
        if was_busy {
            self.result = Some(BotResult::Failed(BotError::Disconnected));
        }
    }

    /// Takes the outcome of the transfer [`Self::start`] started.
    pub fn complete(&mut self, result: Result<usize, UsbError>, now: u64) {
        if !self.in_flight {
            return;
        }
        self.in_flight = false;
        if result == Err(UsbError::Disconnected) {
            self.phase = Phase::Idle;
            self.result = Some(BotResult::Failed(BotError::Disconnected));
            return;
        }
        if result == Err(UsbError::Stall) {
            self.stats.stalls += 1;
        }
        if result == Err(UsbError::Timeout) {
            self.stats.timeouts += 1;
        }
        match self.phase {
            Phase::Idle | Phase::ResetSettle { .. } => {}
            Phase::Control => {
                self.phase = Phase::Idle;
                self.result = Some(BotResult::Control(result));
            }
            Phase::Cbw => match result {
                Ok(CBW_LEN) => {
                    self.phase = if self.expected > 0 {
                        Phase::DataIn
                    } else {
                        Phase::Csw
                    };
                }
                // A CBW the device took only part of has left it somewhere
                // in the middle of a command block.
                Ok(_) => self.recover(BotError::Transport(UsbError::Buffer)),
                Err(UsbError::Timeout) => self.recover(BotError::Timeout),
                Err(error) => self.recover(BotError::Transport(error)),
            },
            Phase::DataIn => match result {
                Ok(n) => {
                    let n = n.min(self.chunk);
                    self.transferred += n;
                    self.stats.bytes_in += n as u64;
                    if n < self.chunk || self.transferred == self.expected {
                        self.phase = Phase::Csw;
                    }
                }
                // Section 6.7.2: the device stalls the data stage to end it
                // early; the CSW still follows once the halt is cleared.
                Err(UsbError::Stall) => self.phase = Phase::ClearDataIn,
                Err(UsbError::Timeout) => self.recover(BotError::Timeout),
                Err(error) => self.recover(BotError::Transport(error)),
            },
            Phase::ClearDataIn | Phase::ClearCsw => match result {
                Ok(_) => self.phase = Phase::Csw,
                Err(error) => self.recover(BotError::Transport(error)),
            },
            Phase::Csw => match result {
                Ok(n) => self.check_csw(n),
                // Section 6.7.2 again: one stalled CSW is cleared and asked
                // for once more.  A second means the device is lost.
                Err(UsbError::Stall) if !self.csw_retried => {
                    self.csw_retried = true;
                    self.phase = Phase::ClearCsw;
                }
                Err(UsbError::Timeout) => self.recover(BotError::Timeout),
                Err(error) => self.recover(BotError::Transport(error)),
            },
            Phase::ResetDevice => match result {
                Ok(_) => {
                    self.phase = Phase::ResetSettle {
                        until: now.saturating_add(RESET_SETTLE_US),
                    }
                }
                Err(error) => self.recovery_failed(error),
            },
            Phase::ResetClearIn => match result {
                Ok(_) => self.phase = Phase::ResetClearOut,
                Err(error) => self.recovery_failed(error),
            },
            Phase::ResetClearOut => match result {
                Ok(_) => {
                    self.phase = Phase::Idle;
                    let failure = self
                        .failure
                        .take()
                        .unwrap_or(BotError::Transport(UsbError::ControllerFault));
                    self.result = Some(BotResult::Failed(failure));
                }
                Err(error) => self.recovery_failed(error),
            },
        }
    }

    fn check_csw(&mut self, received: usize) {
        let csw = Csw::parse(&self.csw[..received.min(CSW_LEN)]).filter(|_| received == CSW_LEN);
        let Some(csw) = csw else {
            self.stats.invalid_csws += 1;
            return self.recover(BotError::InvalidCsw);
        };
        // Section 6.3.2: meaningful only with the command's own tag and a
        // residue no larger than what was asked for.
        if csw.tag != self.tag || csw.residue as usize > self.expected {
            self.stats.invalid_csws += 1;
            return self.recover(BotError::InvalidCsw);
        }
        if csw.status == CswStatus::PhaseError {
            self.stats.phase_errors += 1;
            return self.recover(BotError::PhaseError);
        }
        self.phase = Phase::Idle;
        self.result = Some(BotResult::Command {
            status: csw.status,
            residue: csw.residue,
            transferred: self.transferred,
        });
    }

    /// Section 5.3.4: Mass Storage Reset, then CLEAR_FEATURE(ENDPOINT_HALT)
    /// on the IN and then the OUT endpoint.  `error` is what the caller hears
    /// once that has finished.
    fn recover(&mut self, error: BotError) {
        self.stats.recoveries += 1;
        self.failure = Some(error);
        self.phase = Phase::ResetDevice;
    }

    fn recovery_failed(&mut self, error: UsbError) {
        self.stats.recovery_failures += 1;
        self.phase = Phase::Idle;
        self.failure = None;
        self.result = Some(BotResult::Failed(BotError::RecoveryFailed(error)));
    }
}

#[cfg(test)]
mod tests {
    use super::super::scsi;
    use super::*;

    /// Drives a Bot with a scripted sequence of completions and records what
    /// it asked for.
    fn step(bot: &mut Bot, now: u64, result: Result<usize, UsbError>) -> Transfer {
        let wanted = bot.wanted().expect("a transfer is wanted");
        bot.start(now);
        bot.complete(result, now);
        wanted
    }

    fn csw(bot: &mut Bot, tag: u32, residue: u32, status: CswStatus) -> usize {
        let bytes = Csw {
            tag,
            residue,
            status,
        }
        .to_bytes();
        bot.in_buffer().copy_from_slice(&bytes);
        CSW_LEN
    }

    #[test]
    fn a_read_runs_cbw_data_and_csw_in_chunks() {
        let mut bot = Bot::new(0, 512);
        bot.command(scsi::read_10(0, 2), 1024, 5_000_000).unwrap();
        assert_eq!(step(&mut bot, 0, Ok(31)), Transfer::BulkOut { len: 31 });
        assert_eq!(bot.wanted(), Some(Transfer::BulkIn { len: 512 }));
        bot.start(0);
        bot.in_buffer().fill(0xaa);
        bot.complete(Ok(512), 0);
        bot.start(0);
        bot.in_buffer().fill(0xbb);
        bot.complete(Ok(512), 0);
        assert_eq!(bot.wanted(), Some(Transfer::BulkIn { len: 13 }));
        bot.start(0);
        let n = csw(&mut bot, 1, 0, CswStatus::Passed);
        bot.complete(Ok(n), 0);
        assert_eq!(
            bot.take_result(),
            Some(BotResult::Command {
                status: CswStatus::Passed,
                residue: 0,
                transferred: 1024
            })
        );
        assert_eq!(bot.data()[511], 0xaa);
        assert_eq!(bot.data()[512], 0xbb);
        assert!(!bot.busy());
    }

    #[test]
    fn a_short_transfer_ends_the_data_stage() {
        let mut bot = Bot::new(0, 512);
        bot.command(scsi::inquiry(36), 36, 5_000_000).unwrap();
        step(&mut bot, 0, Ok(31));
        assert_eq!(step(&mut bot, 0, Ok(5)), Transfer::BulkIn { len: 36 });
        assert_eq!(bot.wanted(), Some(Transfer::BulkIn { len: 13 }));
        bot.start(0);
        let n = csw(&mut bot, 1, 31, CswStatus::Passed);
        bot.complete(Ok(n), 0);
        assert_eq!(
            bot.take_result(),
            Some(BotResult::Command {
                status: CswStatus::Passed,
                residue: 31,
                transferred: 5
            })
        );
    }

    #[test]
    fn a_stalled_data_stage_is_cleared_and_the_csw_still_read() {
        let mut bot = Bot::new(0, 512);
        bot.command(scsi::read_10(0, 1), 512, 5_000_000).unwrap();
        step(&mut bot, 0, Ok(31));
        step(&mut bot, 0, Err(UsbError::Stall));
        assert_eq!(
            step(&mut bot, 0, Ok(0)),
            Transfer::ClearHalt { pipe: Pipe::In }
        );
        bot.start(0);
        let n = csw(&mut bot, 1, 512, CswStatus::Failed);
        bot.complete(Ok(n), 0);
        assert_eq!(
            bot.take_result(),
            Some(BotResult::Command {
                status: CswStatus::Failed,
                residue: 512,
                transferred: 0
            })
        );
    }

    /// Runs Reset Recovery to the end, checking the order of its requests.
    fn expect_recovery(bot: &mut Bot, now: u64) {
        assert_eq!(
            step(bot, now, Ok(0)),
            Transfer::Control {
                setup: wire::mass_storage_reset(0)
            }
        );
        assert_eq!(bot.wanted(), None, "settling after the reset");
        bot.poll(now + RESET_SETTLE_US - 1);
        assert_eq!(bot.wanted(), None);
        bot.poll(now + RESET_SETTLE_US);
        assert_eq!(
            step(bot, now, Ok(0)),
            Transfer::ClearHalt { pipe: Pipe::In }
        );
        assert_eq!(
            step(bot, now, Ok(0)),
            Transfer::ClearHalt { pipe: Pipe::Out }
        );
    }

    #[test]
    fn a_csw_stalled_twice_ends_in_reset_recovery() {
        let mut bot = Bot::new(0, 512);
        bot.command(scsi::test_unit_ready(), 0, 5_000_000).unwrap();
        step(&mut bot, 0, Ok(31));
        step(&mut bot, 0, Err(UsbError::Stall));
        step(&mut bot, 0, Ok(0));
        step(&mut bot, 0, Err(UsbError::Stall));
        expect_recovery(&mut bot, 0);
        assert_eq!(
            bot.take_result(),
            Some(BotResult::Failed(BotError::Transport(UsbError::Stall)))
        );
    }

    #[test]
    fn a_bad_csw_is_never_taken_as_success() {
        for (tag, residue, len, label) in [
            (2u32, 0u32, CSW_LEN, "wrong tag"),
            (1, 513, CSW_LEN, "residue beyond the request"),
            (1, 0, CSW_LEN - 1, "short"),
        ] {
            let mut bot = Bot::new(0, 512);
            bot.command(scsi::read_10(0, 1), 512, 5_000_000).unwrap();
            step(&mut bot, 0, Ok(31));
            step(&mut bot, 0, Ok(512));
            bot.start(0);
            csw(&mut bot, tag, residue, CswStatus::Passed);
            bot.complete(Ok(len), 0);
            expect_recovery(&mut bot, 0);
            assert_eq!(
                bot.take_result(),
                Some(BotResult::Failed(BotError::InvalidCsw)),
                "{label}"
            );
        }
    }

    #[test]
    fn a_bad_signature_and_a_phase_error_both_recover() {
        let mut bot = Bot::new(0, 512);
        bot.command(scsi::test_unit_ready(), 0, 5_000_000).unwrap();
        step(&mut bot, 0, Ok(31));
        bot.start(0);
        bot.in_buffer().fill(0);
        bot.complete(Ok(CSW_LEN), 0);
        expect_recovery(&mut bot, 0);
        assert_eq!(
            bot.take_result(),
            Some(BotResult::Failed(BotError::InvalidCsw))
        );

        bot.command(scsi::test_unit_ready(), 0, 5_000_000).unwrap();
        step(&mut bot, 0, Ok(31));
        bot.start(0);
        let n = csw(&mut bot, 2, 0, CswStatus::PhaseError);
        bot.complete(Ok(n), 0);
        expect_recovery(&mut bot, 0);
        assert_eq!(
            bot.take_result(),
            Some(BotResult::Failed(BotError::PhaseError))
        );
        assert_eq!(bot.stats().recoveries, 2);
    }

    #[test]
    fn a_data_stage_that_overruns_into_the_csw_read_is_caught() {
        // The device sends more data than asked; the 13-byte read gets data.
        let mut bot = Bot::new(0, 512);
        bot.command(scsi::read_10(0, 1), 512, 5_000_000).unwrap();
        step(&mut bot, 0, Ok(31));
        step(&mut bot, 0, Ok(512));
        bot.start(0);
        bot.in_buffer().fill(0x55);
        bot.complete(Ok(CSW_LEN), 0);
        expect_recovery(&mut bot, 0);
        assert_eq!(
            bot.take_result(),
            Some(BotResult::Failed(BotError::InvalidCsw))
        );
    }

    #[test]
    fn a_partly_taken_cbw_recovers() {
        let mut bot = Bot::new(0, 512);
        bot.command(scsi::test_unit_ready(), 0, 5_000_000).unwrap();
        step(&mut bot, 0, Ok(30));
        expect_recovery(&mut bot, 0);
        assert!(matches!(bot.take_result(), Some(BotResult::Failed(_))));
    }

    #[test]
    fn the_command_deadline_holds_even_if_nothing_was_started() {
        let mut bot = Bot::new(0, 512);
        bot.command(scsi::test_unit_ready(), 0, 1_000).unwrap();
        bot.poll(999);
        assert_eq!(bot.wanted(), Some(Transfer::BulkOut { len: 31 }));
        bot.poll(1_000);
        expect_recovery(&mut bot, 1_000);
        assert_eq!(
            bot.take_result(),
            Some(BotResult::Failed(BotError::Timeout))
        );
    }

    #[test]
    fn bulk_transfers_carry_the_command_deadline_and_controls_their_own() {
        let mut bot = Bot::new(0, 512);
        bot.command(scsi::test_unit_ready(), 0, 4_000).unwrap();
        assert_eq!(bot.start(100), 4_000);
        bot.complete(Err(UsbError::Timeout), 4_000);
        assert_eq!(bot.start(4_000), 4_000 + CONTROL_TIMEOUT_US);
    }

    #[test]
    fn a_failed_reset_makes_the_interface_unusable() {
        let mut bot = Bot::new(0, 512);
        bot.command(scsi::test_unit_ready(), 0, 5_000_000).unwrap();
        step(&mut bot, 0, Err(UsbError::Transaction));
        step(&mut bot, 0, Err(UsbError::Stall));
        assert_eq!(
            bot.take_result(),
            Some(BotResult::Failed(BotError::RecoveryFailed(UsbError::Stall)))
        );
        assert_eq!(bot.stats().recovery_failures, 1);
    }

    #[test]
    fn a_disconnect_ends_the_command_without_recovery() {
        let mut bot = Bot::new(0, 512);
        bot.command(scsi::read_10(0, 1), 512, 5_000_000).unwrap();
        step(&mut bot, 0, Ok(31));
        step(&mut bot, 0, Err(UsbError::Disconnected));
        assert_eq!(
            bot.take_result(),
            Some(BotResult::Failed(BotError::Disconnected))
        );
        assert!(!bot.busy());
        assert_eq!(bot.stats().recoveries, 0);
    }

    #[test]
    fn abort_reports_a_disconnect_only_when_something_was_running() {
        let mut bot = Bot::new(0, 512);
        bot.abort();
        assert_eq!(bot.take_result(), None);
        bot.command(scsi::test_unit_ready(), 0, 5_000_000).unwrap();
        bot.start(0);
        bot.abort();
        assert_eq!(
            bot.take_result(),
            Some(BotResult::Failed(BotError::Disconnected))
        );
        // A late completion for the aborted transfer is ignored.
        bot.complete(Ok(31), 0);
        assert_eq!(bot.take_result(), None);
    }

    #[test]
    fn control_requests_pass_through() {
        let mut bot = Bot::new(3, 512);
        bot.control(wire::get_max_lun(3)).unwrap();
        bot.start(0);
        assert_eq!(bot.in_buffer().len(), 1);
        bot.in_buffer()[0] = 0;
        bot.complete(Ok(1), 0);
        assert_eq!(bot.take_result(), Some(BotResult::Control(Ok(1))));
        assert!(bot.command(scsi::test_unit_ready(), 0, 1).is_ok());
        assert!(
            bot.control(wire::get_max_lun(3)).is_err(),
            "one thing at a time"
        );
    }

    #[test]
    fn tags_differ_between_commands() {
        let mut bot = Bot::new(0, 512);
        bot.command(scsi::test_unit_ready(), 0, 5_000_000).unwrap();
        let first = bot.out_data()[4..8].to_vec();
        step(&mut bot, 0, Ok(31));
        bot.start(0);
        let n = csw(&mut bot, 1, 0, CswStatus::Passed);
        bot.complete(Ok(n), 0);
        bot.take_result();
        bot.command(scsi::test_unit_ready(), 0, 5_000_000).unwrap();
        assert_ne!(first, bot.out_data()[4..8].to_vec());
    }
}

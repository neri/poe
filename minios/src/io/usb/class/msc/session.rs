//! One mass storage interface: SCSI on top of [`Bot`], and the link to the
//! [`Registry`] through which reads arrive.
//!
//! Like [`Bot`], a session does no I/O of its own.  A backend calls
//! [`MscSession::poll`] from its service poll, performs whatever
//! [`MscSession::wanted`] asks for, and reports back with
//! [`MscSession::complete`].  Everything time-driven — the readiness window,
//! the periodic recheck of an empty drive, the command deadline — advances
//! from those calls, so nothing here waits.
//!
//! Bring-up is GET_MAX_LUN, INQUIRY, TEST UNIT READY (with REQUEST SENSE when
//! it fails) and READ CAPACITY.  Reads are split into commands of at most
//! [`MAX_COMMAND_BYTES`], each retried at most once, and each bounded — with
//! its recovery and retry — by [`REQUEST_BUDGET_US`].

use libusb::UsbError;

use super::bot::{
    Bot, BotError, BotResult, BotStats, COMMAND_TIMEOUT_US, MAX_COMMAND_BYTES, RECOVERY_BUDGET_US,
    Transfer,
};
use super::registry::{DeviceHandle, MediaState, REQUEST_BUDGET_US, Registry};
use super::scsi::{self, Capacity, Inquiry, Sense, SenseClass};
use super::wire::{self, CswStatus};
use crate::io::fs::media::BlockIoError;

/// How long a medium has to become ready after the device appears or after a
/// Unit Attention, before it is reported as not ready.
pub const READY_WINDOW_US: u64 = 10_000_000;
/// Between two TEST UNIT READYs inside that window.
pub const READY_RETRY_US: u64 = 250_000;
/// Between two looks at an empty or not-ready drive once the window is over.
pub const RECHECK_US: u64 = 1_000_000;
/// Consecutive transport failures, each already through Reset Recovery,
/// after which the interface is given up on.
const MAX_TRANSPORT_FAILURES: u8 = 4;
const MAX_INQUIRY_ATTEMPTS: u8 = 3;
/// Unit Attentions answered by retrying straight away.  A device reports one
/// per condition, so a long run of them is a device that never settles.
const MAX_UNIT_ATTENTIONS: u8 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Op {
    MaxLun,
    Inquiry,
    TestUnitReady,
    Capacity10,
    Capacity16,
    Read { blocks: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Activity {
    Idle,
    Wait {
        until: u64,
    },
    Command(Op),
    /// REQUEST SENSE after `Op` failed.
    Sense(Op),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Media {
    /// Before the first TEST UNIT READY.
    Unknown,
    Probing {
        window_end: u64,
    },
    Ready {
        block_size: u32,
        block_count: u64,
    },
    NoMedia {
        recheck_at: u64,
    },
    NotReady {
        recheck_at: u64,
    },
    Failed,
    Unsupported,
}

/// The read being carried out.
#[derive(Clone, Copy, Debug)]
struct Job {
    lba: u64,
    blocks: u64,
    done: u64,
    /// When the current command was first issued; its retry counts against
    /// the same budget.
    attempt_start: u64,
    retried: bool,
}

pub struct MscSession {
    handle: DeviceHandle,
    interface: u8,
    bot: Bot,
    max_lun_known: bool,
    inquiry_done: bool,
    inquiry_attempts: u8,
    media: Media,
    activity: Activity,
    job: Option<Job>,
    /// A reset() is waiting for bring-up to finish.
    reinit: bool,
    /// Whether the medium may have changed since it was last ready, which
    /// decides whether it comes back under the same media generation.
    media_changed: bool,
    last_capacity: Option<(u32, u64)>,
    transport_failures: u8,
    unit_attentions: u8,
    transport_errors: u64,
    read_retries: u64,
    detached: bool,
}

impl MscSession {
    /// `max_chunk` is the backend's largest bulk transfer; see [`Bot::new`].
    pub fn new(handle: DeviceHandle, interface: u8, max_chunk: usize) -> Self {
        Self {
            handle,
            interface,
            bot: Bot::new(interface, max_chunk),
            max_lun_known: false,
            inquiry_done: false,
            inquiry_attempts: 0,
            media: Media::Unknown,
            activity: Activity::Idle,
            job: None,
            reinit: false,
            media_changed: true,
            last_capacity: None,
            transport_failures: 0,
            unit_attentions: 0,
            transport_errors: 0,
            read_retries: 0,
            detached: false,
        }
    }

    pub const fn handle(&self) -> DeviceHandle {
        self.handle
    }

    pub fn bot_stats(&self) -> BotStats {
        self.bot.stats()
    }

    /// True while there is work under way that time alone will not advance
    /// promptly: a command, a read, or the readiness window.
    pub fn busy(&self) -> bool {
        !self.detached
            && (self.bot.busy()
                || self.job.is_some()
                || matches!(self.media, Media::Unknown | Media::Probing { .. }))
    }

    /// True once the interface is beyond use and holds no transfer.
    pub fn is_dead(&self) -> bool {
        matches!(self.media, Media::Failed) && !self.bot.in_flight()
    }

    pub fn wanted(&self) -> Option<Transfer> {
        if self.detached {
            return None;
        }
        self.bot.wanted()
    }

    pub fn start(&mut self, now: u64) -> u64 {
        self.bot.start(now)
    }

    pub fn out_data(&self) -> &[u8] {
        self.bot.out_data()
    }

    pub fn in_buffer(&mut self) -> &mut [u8] {
        self.bot.in_buffer()
    }

    /// Takes the outcome of the transfer last started, and moves on.
    pub fn complete(&mut self, result: Result<usize, UsbError>, now: u64, registry: &mut Registry) {
        self.bot.complete(result, now);
        self.poll(now, registry);
    }

    /// The device has gone.  The backend has already stopped any transfer.
    pub fn detach(&mut self, registry: &mut Registry) {
        if self.detached {
            return;
        }
        self.bot.abort();
        let _ = self.bot.take_result();
        self.detached = true;
        self.job = None;
        self.activity = Activity::Idle;
        self.sync_stats(registry);
        registry.detach(self.handle);
        crate::usb_println!("USB MSC {}: detached", self.handle.index);
    }

    /// Advances whatever does not need a transfer to complete.
    pub fn poll(&mut self, now: u64, registry: &mut Registry) {
        if self.detached {
            return;
        }
        self.bot.poll(now);
        if let Some(result) = self.bot.take_result() {
            self.handle_result(result, now, registry);
            self.sync_stats(registry);
        }
        if let Activity::Wait { until } = self.activity
            && now >= until
        {
            self.activity = Activity::Idle;
        }
        if self.activity == Activity::Idle && !self.bot.busy() {
            self.next(now, registry);
        }
    }

    fn sync_stats(&self, registry: &mut Registry) {
        let bot = self.bot.stats();
        if let Some(stats) = registry.stats_mut(self.handle) {
            stats.commands = bot.commands;
            stats.bus_bytes_in = bot.bytes_in;
            stats.stalls = bot.stalls;
            stats.timeouts = bot.timeouts;
            stats.recoveries = bot.recoveries;
            stats.recovery_failures = bot.recovery_failures;
            stats.invalid_csws = bot.invalid_csws;
            stats.phase_errors = bot.phase_errors;
            stats.transport_errors = self.transport_errors;
            stats.read_retries = self.read_retries;
        }
    }

    fn command(&mut self, op: Op, cdb: scsi::Cdb, len: usize, now: u64) {
        self.command_by(op, cdb, len, now.saturating_add(COMMAND_TIMEOUT_US));
    }

    fn command_by(&mut self, op: Op, cdb: scsi::Cdb, len: usize, deadline: u64) {
        if self.bot.command(cdb, len, deadline).is_ok() {
            self.activity = Activity::Command(op);
        }
    }

    fn wait(&mut self, until: u64) {
        self.activity = Activity::Wait { until };
    }

    /// Decides what to do next, with nothing in flight.
    fn next(&mut self, now: u64, registry: &mut Registry) {
        // Given up on: nothing more goes to the device until it is
        // enumerated again, whatever bring-up had still to do.
        if matches!(self.media, Media::Failed | Media::Unsupported) {
            self.refuse_requests(registry);
            return;
        }
        if !self.max_lun_known {
            if self.bot.control(wire::get_max_lun(self.interface)).is_ok() {
                self.activity = Activity::Command(Op::MaxLun);
            }
            return;
        }
        if !self.inquiry_done {
            self.command(
                Op::Inquiry,
                scsi::inquiry(scsi::INQUIRY_LEN as u16),
                scsi::INQUIRY_LEN,
                now,
            );
            return;
        }
        match self.media {
            Media::Unknown => {
                self.media = Media::Probing {
                    window_end: now.saturating_add(READY_WINDOW_US),
                };
                self.test_unit_ready(now);
            }
            Media::Probing { .. } => self.test_unit_ready(now),
            Media::Ready { .. } => {
                if self.job.is_some() {
                    self.issue_read(now, registry);
                } else {
                    self.take_request(now, registry);
                }
            }
            Media::NoMedia { recheck_at } | Media::NotReady { recheck_at } => {
                self.refuse_requests(registry);
                if now >= recheck_at {
                    self.test_unit_ready(now);
                } else {
                    self.wait(recheck_at);
                }
            }
            Media::Failed | Media::Unsupported => self.refuse_requests(registry),
        }
    }

    fn test_unit_ready(&mut self, now: u64) {
        self.command(Op::TestUnitReady, scsi::test_unit_ready(), 0, now);
    }

    /// Answers requests while the medium cannot be read.
    fn refuse_requests(&mut self, registry: &mut Registry) {
        if !registry.has_queued(self.handle) {
            return;
        }
        let is_reinit = registry.take_queued(self.handle).is_some_and(|r| r.reinit);
        if is_reinit && !matches!(self.media, Media::Failed | Media::Unsupported) {
            // Start over as if just attached; the request finishes once that
            // has come to a conclusion.
            self.reinit = true;
            self.media = Media::Unknown;
            registry.set_state(self.handle, MediaState::Probing);
            return;
        }
        let error = match self.media {
            Media::NoMedia { .. } => BlockIoError::NoMedia,
            _ => BlockIoError::DeviceError,
        };
        registry.finish_request(self.handle, Err(error));
    }

    fn take_request(&mut self, now: u64, registry: &mut Registry) {
        let Media::Ready { .. } = self.media else {
            return;
        };
        let current = match registry.state(self.handle) {
            Some(MediaState::Ready { media_id, .. }) => media_id,
            _ => return,
        };
        let Some(request) = registry.take_queued(self.handle) else {
            return;
        };
        if request.reinit {
            self.reinit = true;
            self.media = Media::Unknown;
            registry.set_state(self.handle, MediaState::Probing);
            return;
        }
        if request.media_id != current {
            registry.finish_request(self.handle, Err(BlockIoError::MediaChanged));
            return;
        }
        self.job = Some(Job {
            lba: request.lba,
            blocks: request.blocks,
            done: 0,
            attempt_start: now,
            retried: false,
        });
        self.issue_read(now, registry);
    }

    fn issue_read(&mut self, now: u64, registry: &mut Registry) {
        let (Some(job), Media::Ready { block_size, .. }) = (self.job, self.media) else {
            return;
        };
        if registry
            .active_request(self.handle)
            .is_none_or(|r| r.abandoned)
        {
            // The caller has gone; do not spend the bus on it.
            self.finish_job(registry, Err(BlockIoError::DeviceError));
            return;
        }
        let per_command = (MAX_COMMAND_BYTES / block_size as usize).max(1) as u64;
        let blocks = (job.blocks - job.done).min(per_command) as u32;
        let Some(cdb) = scsi::read(job.lba + job.done, blocks) else {
            self.finish_job(registry, Err(BlockIoError::InvalidParameter));
            return;
        };
        // The command and whatever recovery follows it have to fit in what
        // is left of this command's budget.
        let budget_end = job
            .attempt_start
            .saturating_add(REQUEST_BUDGET_US - RECOVERY_BUDGET_US);
        let deadline = now.saturating_add(COMMAND_TIMEOUT_US).min(budget_end);
        self.command_by(
            Op::Read { blocks },
            cdb,
            blocks as usize * block_size as usize,
            deadline,
        );
    }

    fn finish_job(&mut self, registry: &mut Registry, result: Result<(), BlockIoError>) {
        self.job = None;
        registry.finish_request(self.handle, result);
    }

    fn handle_result(&mut self, result: BotResult, now: u64, registry: &mut Registry) {
        let activity = core::mem::replace(&mut self.activity, Activity::Idle);
        match (activity, result) {
            (Activity::Command(Op::MaxLun), BotResult::Control(result)) => {
                // A STALL means the device has only LUN 0 (section 3.2).
                let max_lun = match result {
                    Ok(n) if n >= 1 => self.bot.control_data()[0] & 0x0f,
                    _ => 0,
                };
                if let Err(error) = result
                    && error != UsbError::Stall
                {
                    crate::usb_println!(
                        "USB MSC {}: GET_MAX_LUN failed: {:?}",
                        self.handle.index,
                        error
                    );
                }
                self.max_lun_known = true;
                registry.update_info(self.handle, |info| info.max_lun = max_lun);
                if max_lun > 0 {
                    crate::usb_println!(
                        "USB MSC {}: {} LUNs, only LUN 0 is used",
                        self.handle.index,
                        max_lun as u16 + 1
                    );
                }
            }
            (
                Activity::Command(op),
                BotResult::Command {
                    status,
                    residue,
                    transferred,
                },
            ) => match status {
                CswStatus::Passed => self.passed(op, residue, transferred, now, registry),
                _ => {
                    let deadline = now.saturating_add(COMMAND_TIMEOUT_US);
                    if self
                        .bot
                        .command(
                            scsi::request_sense(scsi::SENSE_LEN as u8),
                            scsi::SENSE_LEN,
                            deadline,
                        )
                        .is_ok()
                    {
                        self.activity = Activity::Sense(op);
                    }
                }
            },
            (
                Activity::Sense(op),
                BotResult::Command {
                    status,
                    transferred,
                    ..
                },
            ) => {
                let sense = (status == CswStatus::Passed)
                    .then(|| Sense::parse(&self.bot.data()[..transferred]))
                    .flatten();
                if let Some(sense) = sense
                    && let Some(stats) = registry.stats_mut(self.handle)
                {
                    stats.last_sense = (sense.key, sense.asc, sense.ascq);
                }
                self.failed(op, sense, now, registry);
            }
            (Activity::Command(op) | Activity::Sense(op), BotResult::Failed(error)) => {
                self.transport_failed(op, error, now, registry)
            }
            _ => {}
        }
    }

    fn passed(
        &mut self,
        op: Op,
        residue: u32,
        transferred: usize,
        now: u64,
        registry: &mut Registry,
    ) {
        self.transport_failures = 0;
        match op {
            Op::MaxLun => {}
            Op::Inquiry => {
                let Some(inquiry) = Inquiry::parse(&self.bot.data()[..transferred]) else {
                    self.failed(op, None, now, registry);
                    return;
                };
                self.inquiry_done = true;
                registry.update_info(self.handle, |info| {
                    info.vendor = inquiry.vendor;
                    info.product = inquiry.product;
                });
                crate::usb_println!(
                    "USB MSC {}: \"{}\" \"{}\" type {:#04x}{}",
                    self.handle.index,
                    core::str::from_utf8(&inquiry.vendor)
                        .unwrap_or("?")
                        .trim_end(),
                    core::str::from_utf8(&inquiry.product)
                        .unwrap_or("?")
                        .trim_end(),
                    inquiry.device_type,
                    if inquiry.removable { ", removable" } else { "" }
                );
                if !inquiry.is_direct_access() {
                    self.unsupported("not a direct-access block device", registry);
                }
            }
            Op::TestUnitReady => {
                self.command(
                    Op::Capacity10,
                    scsi::read_capacity_10(),
                    scsi::CAPACITY_10_LEN,
                    now,
                );
            }
            Op::Capacity10 => match Capacity::parse_10(&self.bot.data()[..transferred]) {
                Some(capacity) if capacity.last_lba == Capacity::LBA_10_OVERFLOW => {
                    self.command(
                        Op::Capacity16,
                        scsi::read_capacity_16(scsi::CAPACITY_16_LEN as u32),
                        scsi::CAPACITY_16_LEN,
                        now,
                    );
                }
                Some(capacity) => self.capacity(capacity, registry),
                None => self.failed(op, None, now, registry),
            },
            Op::Capacity16 => match Capacity::parse_16(&self.bot.data()[..transferred]) {
                Some(capacity) => self.capacity(capacity, registry),
                None => self.failed(op, None, now, registry),
            },
            Op::Read { blocks } => {
                let Media::Ready { block_size, .. } = self.media else {
                    return;
                };
                let expected = blocks as usize * block_size as usize;
                if transferred != expected || residue != 0 {
                    // Passed, but not with everything: never taken as a read.
                    crate::usb_println!(
                        "USB MSC {}: READ passed with {} of {} bytes, residue {}",
                        self.handle.index,
                        transferred,
                        expected,
                        residue
                    );
                    self.retry_or_fail(now, registry, BlockIoError::DeviceError);
                    return;
                }
                let Some(done) = self.job.map(|job| job.done) else {
                    return;
                };
                let offset = (done * block_size as u64) as usize;
                let copied = match registry.active_request(self.handle) {
                    Some(request) if offset + expected <= request.data.len() => {
                        request.data[offset..offset + expected].copy_from_slice(self.bot.data());
                        true
                    }
                    _ => false,
                };
                if !copied {
                    self.finish_job(registry, Err(BlockIoError::DeviceError));
                    return;
                }
                let Some(job) = self.job.as_mut() else {
                    return;
                };
                job.done += blocks as u64;
                job.retried = false;
                job.attempt_start = now;
                if job.done >= job.blocks {
                    self.finish_job(registry, Ok(()));
                }
            }
        }
    }

    fn capacity(&mut self, capacity: Capacity, registry: &mut Registry) {
        if !scsi::block_length_supported(capacity.block_length) {
            crate::usb_println!(
                "USB MSC {}: block length {} is not supported",
                self.handle.index,
                capacity.block_length
            );
            self.unsupported("unsupported block length", registry);
            return;
        }
        let Some(block_count) = capacity.block_count() else {
            self.unsupported("capacity out of range", registry);
            return;
        };
        let block_size = capacity.block_length;
        let same_medium =
            !self.media_changed && self.last_capacity == Some((block_size, block_count));
        self.media = Media::Ready {
            block_size,
            block_count,
        };
        self.unit_attentions = 0;
        // A reset() that found the same medium keeps the handles opened on
        // it; anything else is a new medium.
        registry.media_ready(self.handle, block_size, block_count, same_medium);
        self.media_changed = false;
        self.last_capacity = Some((block_size, block_count));
        crate::usb_println!(
            "USB MSC {}: ready, {} blocks of {} bytes",
            self.handle.index,
            block_count,
            block_size
        );
        self.settle_reinit(registry);
    }

    /// Finishes a pending reset() once bring-up has reached a conclusion.
    fn settle_reinit(&mut self, registry: &mut Registry) {
        if self.reinit && !matches!(self.media, Media::Unknown | Media::Probing { .. }) {
            self.reinit = false;
            registry.finish_request(self.handle, Ok(()));
        }
    }

    fn unsupported(&mut self, reason: &'static str, registry: &mut Registry) {
        self.media = Media::Unsupported;
        registry.set_state(self.handle, MediaState::Unsupported(reason));
        self.settle_reinit(registry);
    }

    fn give_up(&mut self, reason: &'static str, registry: &mut Registry) {
        crate::usb_println!("USB MSC {}: giving up: {}", self.handle.index, reason);
        self.media = Media::Failed;
        registry.set_state(self.handle, MediaState::Failed(reason));
        if self.job.is_some() {
            self.finish_job(registry, Err(BlockIoError::DeviceError));
        }
        self.settle_reinit(registry);
    }

    fn no_media(&mut self, now: u64, registry: &mut Registry) {
        if !matches!(self.media, Media::NoMedia { .. }) {
            crate::usb_println!("USB MSC {}: no medium", self.handle.index);
        }
        self.media_changed = true;
        self.media = Media::NoMedia {
            recheck_at: now.saturating_add(RECHECK_US),
        };
        registry.set_state(self.handle, MediaState::NoMedia);
        self.settle_reinit(registry);
    }

    fn not_ready(&mut self, now: u64, registry: &mut Registry) {
        if !matches!(self.media, Media::NotReady { .. }) {
            crate::usb_println!("USB MSC {}: not ready", self.handle.index);
        }
        self.media = Media::NotReady {
            recheck_at: now.saturating_add(RECHECK_US),
        };
        registry.set_state(self.handle, MediaState::NotReady);
        self.settle_reinit(registry);
    }

    /// Back to bring-up with a fresh readiness window, as after a Unit
    /// Attention.
    fn reprobe(&mut self, now: u64, registry: &mut Registry) {
        self.media = Media::Probing {
            window_end: now.saturating_add(READY_WINDOW_US),
        };
        registry.set_state(self.handle, MediaState::Probing);
    }

    /// A command finished with CHECK CONDITION.  `sense` is what REQUEST
    /// SENSE said, if it said anything usable.
    fn failed(&mut self, op: Op, sense: Option<Sense>, now: u64, registry: &mut Registry) {
        let class = sense.map_or(SenseClass::Other, |s| s.class());
        match op {
            Op::MaxLun => {}
            Op::Inquiry => {
                self.inquiry_attempts += 1;
                if self.inquiry_attempts >= MAX_INQUIRY_ATTEMPTS {
                    self.give_up("INQUIRY failed", registry);
                }
            }
            Op::TestUnitReady | Op::Capacity10 | Op::Capacity16 => {
                if op == Op::Capacity16 && class == SenseClass::IllegalRequest {
                    // The capacity does not fit READ CAPACITY(10) and the
                    // device will not say it any other way.
                    self.unsupported("READ CAPACITY(16) not supported", registry);
                    return;
                }
                match class {
                    SenseClass::NoMedium => self.no_media(now, registry),
                    SenseClass::UnitAttention if self.unit_attentions < MAX_UNIT_ATTENTIONS => {
                        // Reported once per event; asking again is the
                        // normal answer, straight away.
                        self.unit_attentions += 1;
                        self.media_changed = true;
                        if !matches!(self.media, Media::Probing { .. }) {
                            self.reprobe(now, registry);
                        }
                    }
                    _ => match self.media {
                        Media::Probing { window_end } if now < window_end => {
                            self.wait(now.saturating_add(READY_RETRY_US));
                        }
                        Media::NoMedia { .. } | Media::NotReady { .. }
                            if sense.is_some_and(|s| {
                                s.asc == scsi::ASC_NOT_READY && s.ascq == 0x01
                            }) =>
                        {
                            // Becoming ready: a medium has just gone in.
                            self.media_changed = true;
                            self.reprobe(now, registry);
                            self.wait(now.saturating_add(READY_RETRY_US));
                        }
                        _ => self.not_ready(now, registry),
                    },
                }
            }
            Op::Read { .. } => match class {
                SenseClass::UnitAttention => {
                    self.media_changed = true;
                    if let Some(stats) = registry.stats_mut(self.handle) {
                        stats.media_changes += 1;
                    }
                    self.finish_job(registry, Err(BlockIoError::MediaChanged));
                    self.reprobe(now, registry);
                }
                SenseClass::NoMedium => {
                    self.finish_job(registry, Err(BlockIoError::NoMedia));
                    self.no_media(now, registry);
                }
                SenseClass::NotReady => {
                    self.finish_job(registry, Err(BlockIoError::DeviceError));
                    self.reprobe(now, registry);
                }
                SenseClass::IllegalRequest | SenseClass::DataProtect => {
                    self.finish_job(registry, Err(BlockIoError::DeviceError));
                }
                _ => self.retry_or_fail(now, registry, BlockIoError::DeviceError),
            },
        }
    }

    /// A command did not produce a status.  Reset Recovery has already run,
    /// unless the device has gone or cannot be recovered.
    fn transport_failed(&mut self, op: Op, error: BotError, now: u64, registry: &mut Registry) {
        crate::usb_println!(
            "USB MSC {}: {:?} failed: {:?}",
            self.handle.index,
            op,
            error
        );
        match error {
            BotError::Disconnected => {
                if self.job.is_some() {
                    self.finish_job(registry, Err(BlockIoError::NoMedia));
                }
                return;
            }
            BotError::RecoveryFailed(_) => {
                self.give_up("reset recovery failed", registry);
                return;
            }
            _ => {}
        }
        self.transport_errors += 1;
        self.transport_failures = self.transport_failures.saturating_add(1);
        if self.transport_failures >= MAX_TRANSPORT_FAILURES {
            self.give_up("repeated transport failures", registry);
            return;
        }
        match op {
            Op::MaxLun => {}
            Op::Inquiry => {
                self.inquiry_attempts += 1;
                if self.inquiry_attempts >= MAX_INQUIRY_ATTEMPTS {
                    self.give_up("INQUIRY failed", registry);
                }
            }
            Op::TestUnitReady | Op::Capacity10 | Op::Capacity16 => match self.media {
                Media::Probing { window_end } if now < window_end => {
                    self.wait(now.saturating_add(READY_RETRY_US));
                }
                _ => self.not_ready(now, registry),
            },
            Op::Read { .. } => self.retry_or_fail(now, registry, BlockIoError::DeviceError),
        }
    }

    /// Retries the current read command once, if its budget still has room
    /// for a whole command and a recovery after it.
    fn retry_or_fail(&mut self, now: u64, registry: &mut Registry, error: BlockIoError) {
        let Some(job) = self.job.as_mut() else {
            return;
        };
        let budget_end = job.attempt_start.saturating_add(REQUEST_BUDGET_US);
        let room = budget_end.saturating_sub(now);
        if !job.retried && room >= COMMAND_TIMEOUT_US + RECOVERY_BUDGET_US {
            job.retried = true;
            self.read_retries += 1;
            return;
        }
        self.finish_job(registry, Err(error));
    }
}

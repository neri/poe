//! A fake BOT disk, and the session and block device driven against it.
//!
//! The disk answers the way section 6.7 of the BOT specification says a
//! device does, and takes injected faults: stalls, bad CSWs, phase errors,
//! timeouts, sense conditions.  The driver loop plays the backend's part of
//! the contract in `bot.rs`: one transfer at a time, each completed exactly
//! once, a timeout reported at the deadline.

use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use libusb::UsbError;

use super::bot::{self, Pipe, Transfer};
use super::fake_disk::{FakeDisk, Fault, pattern};
use super::registry::*;
use super::scsi::{self, sense_key};
use super::session::{MscSession, READY_WINDOW_US};
use super::{wire, *};
use crate::io::fs::media::{BlockDevice, BlockIoError, LBA, MediaId};

/// One session, its disk and a registry, on a shared clock.
struct Rig {
    session: MscSession,
    disk: FakeDisk,
    registry: Registry,
    now: u64,
    /// The deadline of every transfer started, for the budget checks.
    deadlines: Vec<(u64, u64)>,
    clear_halts: Vec<Pipe>,
}

impl Rig {
    fn new(disk: FakeDisk, max_chunk: usize) -> Self {
        let mut registry = Registry::new();
        let handle = registry
            .attach(DeviceInfo::new(Backend::Xhci, 1, None, 0))
            .unwrap();
        Self {
            session: MscSession::new(handle, 0, max_chunk),
            disk,
            registry,
            now: 0,
            deadlines: Vec::new(),
            clear_halts: Vec::new(),
        }
    }

    /// One backend poll: advance the session, perform one transfer if it
    /// wants one, otherwise let time pass.
    fn step(&mut self) {
        self.session.poll(self.now, &mut self.registry);
        let Some(transfer) = self.session.wanted() else {
            self.now += 10_000;
            NOW.with(|n| n.set(self.now));
            return;
        };
        let deadline = self.session.start(self.now);
        self.deadlines.push((self.now, deadline));
        let result = match transfer {
            Transfer::BulkOut { len } => {
                let data = self.session.out_data().to_vec();
                assert_eq!(data.len(), len);
                self.disk.bulk_out(&data)
            }
            Transfer::BulkIn { len } => {
                let buffer = self.session.in_buffer();
                assert_eq!(buffer.len(), len);
                self.disk.bulk_in(buffer)
            }
            Transfer::Control { setup } => {
                let buffer = self.session.in_buffer();
                self.disk.control(setup, buffer)
            }
            Transfer::ClearHalt { pipe } => {
                self.clear_halts.push(pipe);
                let endpoint = match pipe {
                    Pipe::In => 0x81,
                    Pipe::Out => 0x02,
                };
                let ep = libusb::EndpointAddress::from_raw(endpoint).unwrap();
                self.disk.control(wire::clear_endpoint_halt(ep), &mut [])
            }
        };
        if result == Err(UsbError::Timeout) {
            // The backend reports a timeout at the deadline, not before.
            self.now = self.now.max(deadline);
        } else {
            self.now += 125;
        }
        self.session.complete(result, self.now, &mut self.registry);
        NOW.with(|n| n.set(self.now));
    }

    fn state(&self) -> MediaState {
        self.registry.state(self.session.handle()).unwrap()
    }

    fn run_until(&mut self, mut done: impl FnMut(&Self) -> bool) {
        for _ in 0..100_000 {
            if done(self) {
                return;
            }
            self.step();
        }
        panic!("did not settle: {:?} at {}", self.state(), self.now);
    }

    fn ready(&mut self) -> MediaId {
        self.run_until(|r| {
            matches!(
                r.state(),
                MediaState::Ready { .. } | MediaState::Failed(_) | MediaState::Unsupported(_)
            )
        });
        match self.state() {
            MediaState::Ready { media_id, .. } => media_id,
            other => panic!("not ready: {other:?}"),
        }
    }

    fn read(&mut self, lba: u64, blocks: u64) -> Result<Vec<u8>, BlockIoError> {
        let MediaState::Ready {
            media_id,
            block_size,
            ..
        } = self.state()
        else {
            return Err(BlockIoError::NoMedia);
        };
        let handle = self.session.handle();
        self.registry
            .submit_read(handle, media_id, lba, blocks, block_size)?;
        let start = self.now;
        loop {
            if let Some(result) = self.registry.take_result(handle) {
                return result;
            }
            assert!(
                self.now - start <= REQUEST_BUDGET_US * blocks.max(1),
                "a read ran past its budget"
            );
            self.step();
        }
    }
}

fn expected(block_size: u32, lba: u64, blocks: u64) -> Vec<u8> {
    let mut v = Vec::new();
    for block in lba..lba + blocks {
        for offset in 0..block_size as usize {
            v.push(pattern(block_size, block, offset));
        }
    }
    v
}

#[test]
fn bring_up_reaches_ready_through_a_medium_that_is_becoming_ready() {
    let mut disk = FakeDisk::new(512, 2048);
    disk.becoming_ready = 3;
    disk.unit_attention = true;
    let mut rig = Rig::new(disk, 512);
    rig.ready();
    let MediaState::Ready {
        block_size,
        block_count,
        ..
    } = rig.state()
    else {
        unreachable!()
    };
    assert_eq!((block_size, block_count), (512, 2048));
    assert_eq!(rig.disk.controls[0], 0xfe, "GET_MAX_LUN first");
    assert_eq!(rig.disk.opcodes[0], scsi::opcode::INQUIRY);
    let summary = rig.registry.summaries()[0];
    assert_eq!(&summary.info.product, b"BOT DISK        ");
    assert_eq!(summary.info.max_lun, 0, "a stalled GET_MAX_LUN means LUN 0");
    assert!(rig.now < READY_WINDOW_US);
}

#[test]
fn more_luns_are_recorded_and_only_lun_0_is_used() {
    let mut disk = FakeDisk::new(512, 64);
    disk.max_lun = Some(2);
    let mut rig = Rig::new(disk, 512);
    rig.ready();
    assert_eq!(rig.registry.summaries()[0].info.max_lun, 2);
    assert_eq!(rig.read(0, 1).unwrap(), expected(512, 0, 1));
}

#[test]
fn every_supported_block_length_reads_back_its_pattern() {
    for block_size in [512u32, 1024, 2048, 4096] {
        let mut rig = Rig::new(FakeDisk::new(block_size, 64), 4096);
        rig.ready();
        assert_eq!(rig.read(0, 1).unwrap(), expected(block_size, 0, 1));
        assert_eq!(
            rig.read(63, 1).unwrap(),
            expected(block_size, 63, 1),
            "last block"
        );
        assert_eq!(
            rig.read(10, 9).unwrap(),
            expected(block_size, 10, 9),
            "several"
        );
    }
}

#[test]
fn a_large_read_is_split_into_bounded_commands_and_packets() {
    // 64-byte chunks: a Full Speed bulk endpoint on the DWC2, one packet each.
    let mut rig = Rig::new(FakeDisk::new(512, 256), 64);
    rig.ready();
    let before = rig.disk.opcodes.len();
    assert_eq!(rig.read(3, 200).unwrap(), expected(512, 3, 200));
    let reads = rig.disk.opcodes[before..]
        .iter()
        .filter(|&&op| op == scsi::opcode::READ_10)
        .count();
    assert_eq!(reads, 2, "64 KiB per command: 128 + 72 blocks");
}

#[test]
fn an_unsupported_block_length_or_device_type_is_refused_by_name() {
    let mut rig = Rig::new(FakeDisk::new(520, 64), 512);
    rig.run_until(|r| !matches!(r.state(), MediaState::Probing));
    assert!(matches!(rig.state(), MediaState::Unsupported(_)));

    let mut disk = FakeDisk::new(512, 64);
    disk.device_type = 5;
    let mut rig = Rig::new(disk, 512);
    rig.run_until(|r| !matches!(r.state(), MediaState::Probing));
    assert!(matches!(rig.state(), MediaState::Unsupported(_)));
    assert!(
        !rig.disk.opcodes.contains(&scsi::opcode::READ_CAPACITY_10),
        "a CD-ROM is not asked for a block capacity"
    );
}

#[test]
fn a_capacity_past_2_tib_uses_the_sixteen_byte_commands() {
    let blocks = (1u64 << 32) + 100;
    let mut disk = FakeDisk::new(512, blocks);
    disk.large = true;
    let mut rig = Rig::new(disk, 512);
    rig.ready();
    let MediaState::Ready { block_count, .. } = rig.state() else {
        unreachable!()
    };
    assert_eq!(block_count, blocks, "not truncated to 32 bits");
    assert_eq!(
        rig.read(blocks - 2, 2).unwrap(),
        expected(512, blocks - 2, 2)
    );
    assert_eq!(rig.disk.opcodes.last(), Some(&scsi::opcode::READ_16));
    assert_eq!(rig.read(0, 1).unwrap(), expected(512, 0, 1));
    assert_eq!(rig.disk.opcodes.last(), Some(&scsi::opcode::READ_10));
}

#[test]
fn a_large_device_without_read_capacity_16_is_not_given_a_made_up_size() {
    let mut disk = FakeDisk::new(512, 1 << 33);
    disk.large = true;
    disk.rc16 = false;
    let mut rig = Rig::new(disk, 512);
    rig.run_until(|r| !matches!(r.state(), MediaState::Probing));
    assert!(matches!(rig.state(), MediaState::Unsupported(_)));
}

#[test]
fn an_empty_drive_is_rechecked_and_a_new_medium_gets_a_new_generation() {
    let mut disk = FakeDisk::new(512, 64);
    disk.medium = false;
    let mut rig = Rig::new(disk, 512);
    rig.run_until(|r| r.state() == MediaState::NoMedia);
    let tur = |r: &Rig| {
        r.disk
            .opcodes
            .iter()
            .filter(|&&o| o == scsi::opcode::TEST_UNIT_READY)
            .count()
    };
    let polls = tur(&rig);
    let start = rig.now;
    rig.run_until(|r| r.now > start + 3_500_000);
    let rechecks = tur(&rig) - polls;
    assert!(
        (2..=5).contains(&rechecks),
        "about one TEST UNIT READY a second, got {rechecks}"
    );
    rig.disk.medium = true;
    rig.disk.unit_attention = true;
    let first = rig.ready();

    // Swap the medium: the old generation must not carry over.
    rig.disk.unit_attention = true;
    assert_eq!(rig.read(0, 1), Err(BlockIoError::MediaChanged));
    let second = rig.ready();
    assert_ne!(first, second);
    assert_eq!(rig.read(0, 1).unwrap(), expected(512, 0, 1));
}

#[test]
fn a_medium_error_is_retried_once() {
    let mut rig = Rig::new(FakeDisk::new(512, 64), 512);
    rig.ready();
    rig.disk
        .faults
        .push_back(Fault::ReadSense(sense_key::MEDIUM_ERROR, 0x11, 0));
    assert_eq!(rig.read(5, 2).unwrap(), expected(512, 5, 2));
    rig.disk
        .faults
        .push_back(Fault::ReadSense(sense_key::MEDIUM_ERROR, 0x11, 0));
    rig.disk
        .faults
        .push_back(Fault::ReadSense(sense_key::MEDIUM_ERROR, 0x11, 0));
    assert_eq!(rig.read(5, 2), Err(BlockIoError::DeviceError));
    assert_eq!(rig.registry.summaries()[0].stats.last_sense, (3, 0x11, 0));
    // An illegal request is not something a retry fixes.
    rig.disk
        .faults
        .push_back(Fault::ReadSense(sense_key::ILLEGAL_REQUEST, 0x24, 0));
    let before = rig.disk.opcodes.len();
    assert_eq!(rig.read(5, 2), Err(BlockIoError::DeviceError));
    let reads = rig.disk.opcodes[before..]
        .iter()
        .filter(|&&op| op == scsi::opcode::READ_10)
        .count();
    assert_eq!(reads, 1);
    // And the device is still usable.
    assert_eq!(rig.read(6, 1).unwrap(), expected(512, 6, 1));
}

#[test]
fn transport_faults_recover_and_the_read_is_retried() {
    for fault in [
        Fault::StallCbw,
        Fault::ShortCbw,
        Fault::StallData,
        Fault::StallCswTwice,
        Fault::BadTag,
        Fault::BadSignature,
        Fault::PhaseError,
        Fault::Timeout,
        Fault::ShortRead,
        Fault::Residue,
    ] {
        let mut rig = Rig::new(FakeDisk::new(512, 64), 512);
        rig.ready();
        let recoveries = rig.session.bot_stats().recoveries;
        rig.disk.faults.push_back(fault);
        let start = rig.now;
        assert_eq!(rig.read(7, 3).unwrap(), expected(512, 7, 3), "{fault:?}");
        assert!(
            rig.now - start <= REQUEST_BUDGET_US,
            "{fault:?} took {}",
            rig.now - start
        );
        let recovered = rig.session.bot_stats().recoveries > recoveries;
        let expect_recovery =
            !matches!(fault, Fault::StallData | Fault::ShortRead | Fault::Residue);
        assert_eq!(recovered, expect_recovery, "{fault:?}");
        assert!(rig.disk.faults.is_empty(), "{fault:?} was not exercised");
    }
}

#[test]
fn a_stall_on_the_first_csw_is_cleared_without_reset_recovery() {
    let mut rig = Rig::new(FakeDisk::new(512, 64), 512);
    rig.ready();
    rig.disk.faults.push_back(Fault::StallCsw);
    assert_eq!(rig.read(0, 1).unwrap(), expected(512, 0, 1));
    assert_eq!(rig.session.bot_stats().recoveries, 0);
    assert_eq!(rig.clear_halts, [Pipe::In]);
}

#[test]
fn a_read_that_keeps_timing_out_fails_within_its_budget() {
    let mut rig = Rig::new(FakeDisk::new(512, 64), 512);
    rig.ready();
    for _ in 0..8 {
        rig.disk.faults.push_back(Fault::Timeout);
    }
    let start = rig.now;
    assert_eq!(rig.read(0, 1), Err(BlockIoError::DeviceError));
    assert!(
        rig.now - start <= REQUEST_BUDGET_US,
        "took {}",
        rig.now - start
    );
    for &(started, deadline) in &rig.deadlines {
        assert!(deadline - started <= bot::COMMAND_TIMEOUT_US);
    }
}

#[test]
fn a_failed_reset_recovery_retires_the_interface() {
    let mut rig = Rig::new(FakeDisk::new(512, 64), 512);
    rig.ready();
    rig.disk.faults.push_back(Fault::BadSignature);
    rig.disk.faults.push_back(Fault::ResetStalls);
    assert_eq!(rig.read(0, 1), Err(BlockIoError::DeviceError));
    assert!(matches!(rig.state(), MediaState::Failed(_)));
    let transfers = rig.disk.bulk_transfers;
    for _ in 0..1000 {
        rig.step();
    }
    assert_eq!(rig.disk.bulk_transfers, transfers, "no endless resets");
    assert!(rig.session.is_dead());
}

#[test]
fn a_detach_in_the_middle_of_a_read_fails_it_with_no_media() {
    let mut rig = Rig::new(FakeDisk::new(512, 256), 512);
    let media_id = rig.ready();
    let handle = rig.session.handle();
    rig.registry
        .submit_read(handle, media_id, 0, 64, 512)
        .unwrap();
    for _ in 0..20 {
        rig.step();
    }
    rig.session.detach(&mut rig.registry);
    assert_eq!(
        rig.registry.take_result(handle),
        Some(Err(BlockIoError::NoMedia))
    );
    assert_eq!(rig.session.wanted(), None);
    assert_eq!(rig.state(), MediaState::Detached);
}

#[test]
fn every_transfer_has_a_bounded_deadline() {
    let mut rig = Rig::new(FakeDisk::new(512, 64), 512);
    rig.ready();
    rig.read(0, 8).unwrap();
    for &(started, deadline) in &rig.deadlines {
        let span = deadline - started;
        assert!(span <= bot::COMMAND_TIMEOUT_US, "{span}");
    }
}

// ---- the block device ----------------------------------------------------

struct TestHost {
    rig: Rc<RefCell<Rig>>,
    in_service: bool,
    polls: Rc<RefCell<u64>>,
}

impl BlockHost for TestHost {
    fn in_service(&self) -> bool {
        self.in_service
    }
    fn poll(&mut self) {
        *self.polls.borrow_mut() += 1;
        self.rig.borrow_mut().step();
    }
    fn with_registry<R>(&mut self, f: impl FnOnce(&mut Registry) -> R) -> R {
        f(&mut self.rig.borrow_mut().registry)
    }
}

thread_local! {
    /// The rig's clock, readable while the rig itself is borrowed.
    static NOW: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
}

fn rig_clock() -> u64 {
    NOW.with(|n| n.get())
}

fn block_device(disk: FakeDisk) -> (UsbBlockDevice<TestHost>, Rc<RefCell<Rig>>, Rc<RefCell<u64>>) {
    let rig = Rc::new(RefCell::new(Rig::new(disk, 512)));
    rig.borrow_mut().ready();
    rig.borrow_mut().registry.set_clock(rig_clock);
    let polls = Rc::new(RefCell::new(0));
    let host = TestHost {
        rig: rig.clone(),
        in_service: false,
        polls: polls.clone(),
    };
    (UsbBlockDevice::open_on(host, 0).unwrap(), rig, polls)
}

#[test]
fn the_block_device_reads_and_refuses_what_it_must() {
    let (mut dev, rig, polls) = block_device(FakeDisk::new(512, 100));
    assert_eq!(dev.media_info().block_size, 512);
    assert_eq!(dev.media_info().block_count, LBA(100));

    let mut buffer = vec![0u8; 1024];
    dev.read(LBA(98), &mut buffer).unwrap();
    assert_eq!(buffer, expected(512, 98, 2));

    let transfers = rig.borrow().disk.bulk_transfers;
    assert_eq!(
        dev.write(LBA(0), &buffer),
        Err(BlockIoError::WriteProtected)
    );
    assert_eq!(dev.flush(), Ok(()));
    assert_eq!(dev.read(LBA(0), &mut []), Ok(()), "zero length");
    assert_eq!(
        dev.read(LBA(0), &mut [0u8; 100]),
        Err(BlockIoError::BadBufferSize)
    );
    assert_eq!(
        dev.read(LBA(99), &mut buffer),
        Err(BlockIoError::InvalidParameter)
    );
    assert_eq!(
        dev.read(LBA(u64::MAX), &mut buffer),
        Err(BlockIoError::InvalidParameter)
    );
    assert_eq!(
        rig.borrow().disk.bulk_transfers,
        transfers,
        "none of those reached the bus"
    );
    let _ = polls;
}

#[test]
fn a_read_from_inside_a_service_poll_is_refused_at_once() {
    let (mut dev, rig, polls) = block_device(FakeDisk::new(512, 100));
    dev.host.in_service = true;
    let before = *polls.borrow();
    assert_eq!(
        dev.read(LBA(0), &mut [0u8; 512]),
        Err(BlockIoError::DeviceError)
    );
    assert_eq!(*polls.borrow(), before, "it did not wait");
    let handle = rig.borrow().session.handle();
    assert!(
        !rig.borrow().registry.has_queued(handle),
        "and queued nothing"
    );
}

#[test]
fn old_handles_see_no_media_after_a_detach_and_media_changed_after_a_swap() {
    let (mut dev, rig, _) = block_device(FakeDisk::new(512, 100));
    rig.borrow_mut().disk.unit_attention = true;
    assert_eq!(
        dev.read(LBA(0), &mut [0u8; 512]),
        Err(BlockIoError::MediaChanged)
    );
    rig.borrow_mut().ready();
    assert_eq!(
        dev.read(LBA(0), &mut [0u8; 512]),
        Err(BlockIoError::MediaChanged)
    );
    let mut fresh = UsbBlockDevice::open_on(
        TestHost {
            rig: rig.clone(),
            in_service: false,
            polls: Rc::new(RefCell::new(0)),
        },
        0,
    )
    .unwrap();
    let mut buffer = [0u8; 512];
    fresh.read(LBA(1), &mut buffer).unwrap();
    assert_eq!(buffer.to_vec(), expected(512, 1, 1));
    {
        let mut r = rig.borrow_mut();
        let Rig {
            session, registry, ..
        } = &mut *r;
        session.detach(registry);
    }
    assert_eq!(fresh.read(LBA(1), &mut buffer), Err(BlockIoError::NoMedia));
    assert_eq!(
        fresh.read(LBA(0), &mut []),
        Err(BlockIoError::NoMedia),
        "even for zero length"
    );
}

#[test]
fn reset_finds_the_same_medium_and_keeps_the_handle() {
    let (mut dev, rig, _) = block_device(FakeDisk::new(512, 100));
    dev.reset().unwrap();
    let mut buffer = [0u8; 512];
    dev.read(LBA(3), &mut buffer).unwrap();
    assert_eq!(buffer.to_vec(), expected(512, 3, 1));
    rig.borrow_mut().disk.unit_attention = true;
    dev.reset().unwrap();
    assert_eq!(
        dev.read(LBA(3), &mut buffer),
        Err(BlockIoError::MediaChanged)
    );
}

#[test]
fn a_caller_that_gives_up_leaves_nothing_to_be_written_into() {
    let (mut dev, rig, _) = block_device(FakeDisk::new(512, 100));
    // The disk never answers again.
    for _ in 0..64 {
        rig.borrow_mut().disk.faults.push_back(Fault::Timeout);
    }
    rig.borrow_mut().disk.faults.push_back(Fault::ResetStalls);
    let result = dev.read(LBA(0), &mut [0u8; 512]);
    assert!(result.is_err());
    let handle = rig.borrow().session.handle();
    assert!(rig.borrow_mut().registry.active_request(handle).is_none());
}

// ---- finding interfaces --------------------------------------------------

fn config(interfaces: &[&[u8]]) -> Vec<u8> {
    let mut body = Vec::new();
    for i in interfaces {
        body.extend_from_slice(i);
    }
    let total = 9 + body.len();
    let mut c = vec![
        9,
        2,
        total as u8,
        (total >> 8) as u8,
        interfaces.len() as u8,
        1,
        0,
        0x80,
        50,
    ];
    c.extend(body);
    c
}

fn interface(
    number: u8,
    alternate: u8,
    class: u8,
    sub: u8,
    proto: u8,
    endpoints: &[[u8; 7]],
) -> Vec<u8> {
    let mut v = vec![
        9,
        4,
        number,
        alternate,
        endpoints.len() as u8,
        class,
        sub,
        proto,
        0,
    ];
    for e in endpoints {
        v.extend_from_slice(e);
    }
    v
}

const fn bulk(address: u8, size: u16) -> [u8; 7] {
    [7, 5, address, 2, size as u8, (size >> 8) as u8, 0]
}

#[test]
fn finds_bot_interfaces_among_others() {
    let hid = interface(0, 0, 3, 1, 1, &[[7, 5, 0x83, 3, 8, 0, 10]]);
    let msc = interface(1, 0, 8, 6, 0x50, &[bulk(0x81, 512), bulk(0x02, 512)]);
    let alt = interface(1, 1, 8, 6, 0x62, &[bulk(0x83, 512), bulk(0x04, 512)]);
    let found = find_interfaces(&config(&[&hid, &msc, &alt])).unwrap();
    assert_eq!(found.interfaces.len(), 1);
    let i = found.interfaces[0];
    assert_eq!(i.number, 1);
    assert_eq!(i.bulk_in.address.raw(), 0x81);
    assert_eq!(i.bulk_out.address.raw(), 0x02);
    assert!(
        found.rejected.is_empty(),
        "alternate settings are not interfaces"
    );
}

#[test]
fn refuses_malformed_or_foreign_mass_storage_by_name() {
    let cases: [(Vec<u8>, &str); 6] = [
        (interface(0, 0, 8, 6, 0x50, &[bulk(0x81, 512)]), "missing"),
        (
            interface(
                0,
                0,
                8,
                6,
                0x50,
                &[bulk(0x81, 512), bulk(0x82, 512), bulk(0x02, 512)],
            ),
            "two bulk",
        ),
        (
            interface(0, 0, 8, 6, 0x62, &[bulk(0x81, 512), bulk(0x02, 512)]),
            "UAS",
        ),
        (
            interface(
                0,
                0,
                8,
                4,
                0x00,
                &[bulk(0x81, 64), bulk(0x02, 64), [7, 5, 0x83, 3, 2, 0, 1]],
            ),
            "SCSI",
        ),
        (
            interface(0, 0, 8, 6, 0x01, &[bulk(0x81, 64), bulk(0x02, 64)]),
            "Bulk-Only",
        ),
        (
            interface(0, 0, 8, 6, 0x50, &[bulk(0x81, 100), bulk(0x02, 64)]),
            "packet size",
        ),
    ];
    for (body, reason) in cases {
        let found = find_interfaces(&config(&[&body])).unwrap();
        assert!(found.interfaces.is_empty(), "{reason}");
        assert_eq!(found.rejected.len(), 1, "{reason}");
        assert!(
            found.rejected[0].reason.contains(reason),
            "{} vs {reason}",
            found.rejected[0].reason
        );
    }
}

#[test]
fn superspeed_endpoints_carry_their_companion_burst() {
    let companion = |burst: u8| [6u8, 0x30, burst, 0, 0, 0];
    let mut body = interface(0, 0, 8, 6, 0x50, &[]);
    body[4] = 2;
    body.extend_from_slice(&bulk(0x81, 1024));
    body.extend_from_slice(&companion(15));
    body.extend_from_slice(&bulk(0x02, 1024));
    body.extend_from_slice(&companion(3));
    let found = find_interfaces(&config(&[&body])).unwrap();
    assert_eq!(found.interfaces.len(), 1, "{:?}", found.rejected);
    let i = found.interfaces[0];
    assert_eq!(i.bulk_in.max_packet_size, 1024);
    assert_eq!((i.burst_in, i.burst_out), (15, 3));

    // A companion claiming more than 16 packets per burst is malformed.
    let mut bad = interface(0, 0, 8, 6, 0x50, &[bulk(0x81, 1024)]);
    bad.extend_from_slice(&companion(16));
    bad.extend_from_slice(&bulk(0x02, 1024));
    let found = find_interfaces(&config(&[&bad])).unwrap();
    assert!(found.interfaces.is_empty());
    assert!(found.rejected[0].reason.contains("burst"));

    // Below SuperSpeed there are no companions and the burst stays zero.
    let hs = interface(0, 0, 8, 6, 0x50, &[bulk(0x81, 512), bulk(0x02, 512)]);
    let i = find_interfaces(&config(&[&hs])).unwrap().interfaces[0];
    assert_eq!((i.burst_in, i.burst_out), (0, 0));
}

#[test]
fn a_broken_chain_or_too_many_interfaces() {
    let mut c = config(&[&interface(
        0,
        0,
        8,
        6,
        0x50,
        &[bulk(0x81, 64), bulk(0x02, 64)],
    )]);
    c.push(9);
    assert_eq!(find_interfaces(&c), Err(UsbError::InvalidDescriptor));
    assert_eq!(find_interfaces(&[9, 2]), Err(UsbError::InvalidDescriptor));
    let many: Vec<Vec<u8>> = (0..5)
        .map(|n| interface(n, 0, 8, 6, 0x50, &[bulk(0x81 + n, 64), bulk(0x01 + n, 64)]))
        .collect();
    let refs: Vec<&[u8]> = many.iter().map(|v| v.as_slice()).collect();
    let found = find_interfaces(&config(&refs)).unwrap();
    assert_eq!(found.interfaces.len(), MAX_INTERFACES_PER_DEVICE);
    assert_eq!(found.rejected.len(), 1);
}

#[test]
fn a_device_given_up_on_during_bring_up_is_left_alone() {
    // What a Raspberry Pi 3 did: INQUIRY timed out, and so did the Mass
    // Storage Reset after it.  Bring-up must stop there, not start over.
    let mut disk = FakeDisk::new(512, 64);
    disk.faults.push_back(Fault::Timeout);
    disk.faults.push_back(Fault::ResetStalls);
    let mut rig = Rig::new(disk, 512);
    rig.run_until(|r| matches!(r.state(), MediaState::Failed(_)));
    let (bulk, controls) = (rig.disk.bulk_transfers, rig.disk.controls.len());
    for _ in 0..2_000 {
        rig.step();
    }
    assert_eq!(rig.disk.bulk_transfers, bulk, "no more commands");
    assert_eq!(rig.disk.controls.len(), controls, "no more resets");
    assert!(!rig.session.busy());
}

#[test]
fn a_caller_retrying_a_failed_device_still_lets_the_unplug_be_seen() {
    // usbbench on a Raspberry Pi 3 and 4: the stick was pulled mid-read,
    // Reset Recovery failed, and every later read was refused at once — so
    // the loop never gave the USB service a turn to notice the unplug.
    let (mut dev, rig, polls) = block_device(FakeDisk::new(512, 100));
    rig.borrow_mut().disk.faults.push_back(Fault::BadSignature);
    rig.borrow_mut().disk.faults.push_back(Fault::ResetStalls);
    let mut buffer = [0u8; 512];
    assert_eq!(
        dev.read(LBA(0), &mut buffer),
        Err(BlockIoError::DeviceError)
    );
    let before = *polls.borrow();
    assert_eq!(
        dev.read(LBA(0), &mut buffer),
        Err(BlockIoError::DeviceError)
    );
    assert!(*polls.borrow() > before, "a refused read still polls");
    {
        let mut r = rig.borrow_mut();
        let Rig {
            session, registry, ..
        } = &mut *r;
        session.detach(registry);
    }
    assert_eq!(dev.read(LBA(0), &mut buffer), Err(BlockIoError::NoMedia));
}

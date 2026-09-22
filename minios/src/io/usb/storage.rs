//! Mass storage on the [`super::UsbManager`]: the DWC2 half of the transfer
//! contract in `class/msc/bot.rs`.
//!
//! The session asks for whole BOT stages; this sends them one USB packet at a
//! time.  That is deliberate, and follows what the DWC2 driver of the sibling
//! tab5 project measured its way to: in buffer DMA mode a non-periodic
//! channel halts on a NAK, and a multi-packet transfer that halts part-way
//! reports neither how many packets went through nor which data toggle the
//! endpoint is on.  With one packet per transfer both are exact — the packet
//! either moved and the toggle flips, or it did not and it is sent again
//! with the same PID — and a lost handshake can only produce a duplicate the
//! device discards.
//!
//! What this keeps per interface is what a DWC2 channel does not: the data
//! toggle of each bulk endpoint, and the offset into the stage in progress.

use alloc::boxed::Box;

use libusb::{
    DataPid, Direction, EndpointAddress, SetupPacket, TransferType, UsbAddress, UsbError,
};

use super::class::msc::bot::{Pipe, Transfer};
use super::class::msc::registry::{self, Registry};
use super::class::msc::{MscInterface, MscSession, wire};
use super::hcd::{HostController, TransferCompletion, TransferRequest, TransferToken, UsbRoute};

/// Largest single transfer the DWC2 channels take; see `dwc2::DMA_BUFFER_SIZE`.
pub const MAX_PACKET: usize = 512;
/// A packet that failed with a transaction error is sent again, with the
/// same PID, this many times before the error reaches BOT.
const PACKET_RETRIES: u8 = 3;

/// The bulk stage being sent packet by packet.
#[derive(Clone, Copy, Debug)]
struct Stage {
    pipe: Pipe,
    len: usize,
    done: usize,
    /// The packet on the controller, if one is: its token and length.
    packet: Option<(TransferToken, usize)>,
    retries: u8,
    /// NAKs and transaction errors met so far, for the diagnostic line.
    naks: u32,
    errors: u32,
    deadline: u64,
}

/// A request on endpoint zero the manager's control pipe is carrying for a
/// mass storage interface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlOwner {
    pub handle: registry::DeviceHandle,
    /// Set for CLEAR_FEATURE(ENDPOINT_HALT): the endpoint whose toggle goes
    /// back to DATA0 once the device has taken it.
    pub clears: Option<Pipe>,
}

/// One mass storage interface on the DWC2 bus.
pub struct Storage {
    pub address: UsbAddress,
    pub route: UsbRoute,
    pub ep0_max_packet: u16,
    interface: MscInterface,
    toggle_in: DataPid,
    toggle_out: DataPid,
    stage: Option<Stage>,
    /// What the channel reads from and writes to.  Boxed so that its address
    /// does not change while a transfer holds it.
    bounce: Box<[u8; MAX_PACKET]>,
    pub session: MscSession,
}

impl Storage {
    pub fn new(
        address: UsbAddress,
        route: UsbRoute,
        ep0_max_packet: u16,
        interface: MscInterface,
        handle: registry::DeviceHandle,
    ) -> Self {
        Self {
            address,
            route,
            ep0_max_packet,
            interface,
            toggle_in: DataPid::Data0,
            toggle_out: DataPid::Data0,
            stage: None,
            bounce: Box::new([0; MAX_PACKET]),
            session: MscSession::new(handle, interface.number, MAX_PACKET),
        }
    }

    pub fn handle(&self) -> registry::DeviceHandle {
        self.session.handle()
    }

    /// True while a packet of this interface is on the controller.
    pub fn owns(&self, token: TransferToken) -> bool {
        self.stage
            .and_then(|stage| stage.packet)
            .is_some_and(|(owned, _)| owned == token)
    }

    /// True while a bulk stage is under way, packet on the controller or not.
    pub fn bulk_active(&self) -> bool {
        self.stage.is_some()
    }

    /// True while there is anything for the manager to keep polling for.
    pub fn busy(&self) -> bool {
        self.stage.is_some() || self.session.busy()
    }

    fn endpoint(&self, pipe: Pipe) -> (EndpointAddress, u16) {
        let endpoint = match pipe {
            Pipe::In => self.interface.bulk_in,
            Pipe::Out => self.interface.bulk_out,
        };
        (endpoint.address, endpoint.max_packet_size)
    }

    /// Stops whatever is on the controller, for a device that has gone.  The
    /// channel is left to retire; nothing is copied out of it again.
    pub fn detach(&mut self, hcd: &mut dyn HostController, registry: &mut Registry) {
        if let Some((token, _)) = self.stage.take().and_then(|stage| stage.packet) {
            let _ = hcd.cancel(token);
        }
        self.session.detach(registry);
    }

    /// Advances the session and starts its next bulk stage, if
    /// `bulk_allowed`.  Returns the control request it wants, if any, for the
    /// manager to carry when its control pipe is free.
    ///
    /// The manager keeps bulk stages and control transfers apart: a stage
    /// starts only while no control transfer is running, and a control
    /// transfer only while no stage is.  The keyboard's interrupt polls
    /// still run beside either, as they always have.
    pub fn poll(
        &mut self,
        hcd: &mut dyn HostController,
        now: u64,
        registry: &mut Registry,
        bulk_allowed: bool,
    ) -> Option<Transfer> {
        self.session.poll(now, registry);
        if let Some(stage) = self.stage {
            if stage.packet.is_none() {
                // The controller had no free channel last time.
                self.send_packet(hcd, now, registry);
            }
            return None;
        }
        let transfer = self.session.wanted()?;
        let pipe = match transfer {
            Transfer::BulkOut { .. } => Pipe::Out,
            Transfer::BulkIn { .. } => Pipe::In,
            control => return Some(control),
        };
        if !bulk_allowed {
            return None;
        }
        let len = match transfer {
            Transfer::BulkOut { len } | Transfer::BulkIn { len } => len,
            _ => 0,
        };
        let deadline = self.session.start(now);
        self.stage = Some(Stage {
            pipe,
            len,
            done: 0,
            packet: None,
            retries: 0,
            naks: 0,
            errors: 0,
            deadline,
        });
        self.send_packet(hcd, now, registry);
        None
    }

    /// Puts the next packet of the stage on the controller.
    fn send_packet(&mut self, hcd: &mut dyn HostController, now: u64, registry: &mut Registry) {
        let Some(mut stage) = self.stage else { return };
        if now >= stage.deadline {
            return self.finish(hcd, Err(UsbError::Timeout), now, registry);
        }
        let (endpoint, max_packet) = self.endpoint(stage.pipe);
        let packet = (stage.len - stage.done)
            .min(max_packet as usize)
            .min(MAX_PACKET);
        let (direction, pid) = match stage.pipe {
            Pipe::In => (Direction::In, self.toggle_in),
            Pipe::Out => {
                let data = &self.session.out_data()[stage.done..stage.done + packet];
                self.bounce[..packet].copy_from_slice(data);
                (Direction::Out, self.toggle_out)
            }
        };
        let submitted = hcd.submit(TransferRequest {
            address: self.address,
            endpoint,
            transfer_type: TransferType::Bulk,
            direction,
            route: self.route,
            max_packet_size: max_packet,
            pid,
            buffer: &mut self.bounce[..packet],
            deadline_us: stage.deadline,
        });
        match submitted {
            Ok(token) => {
                stage.packet = Some((token, packet));
                self.stage = Some(stage);
            }
            // Every channel is busy; try again on the next poll.
            Err(UsbError::ResourceExhausted) => {}
            Err(error) => self.finish(hcd, Err(error), now, registry),
        }
    }

    /// Takes the completion of this interface's packet.
    pub fn complete(
        &mut self,
        hcd: &mut dyn HostController,
        completion: TransferCompletion,
        now: u64,
        registry: &mut Registry,
    ) {
        let Some(mut stage) = self.stage else { return };
        let Some((_, packet)) = stage.packet.take() else {
            return;
        };
        match completion.result {
            Ok(moved) => {
                stage.retries = 0;
                // One packet went through, a zero-length one included, so
                // the toggle moves on exactly once.
                let toggle = match stage.pipe {
                    Pipe::In => &mut self.toggle_in,
                    Pipe::Out => &mut self.toggle_out,
                };
                *toggle = toggle.toggled();
                let moved = moved.min(packet);
                if stage.pipe == Pipe::In {
                    let target = self.session.in_buffer();
                    if let Some(slot) = target.get_mut(stage.done..stage.done + moved) {
                        slot.copy_from_slice(&self.bounce[..moved]);
                    }
                } else if moved != packet {
                    // A device that took part of an OUT packet: the offset
                    // cannot be advanced over bytes it never received.
                    self.stage = Some(stage);
                    return self.finish(hcd, Err(UsbError::Buffer), now, registry);
                }
                stage.done += moved;
                self.stage = Some(stage);
                if moved < packet || stage.done >= stage.len {
                    self.finish(hcd, Ok(stage.done), now, registry);
                } else {
                    self.send_packet(hcd, now, registry);
                }
            }
            // Nothing moved; the same packet goes again with the same PID.
            Err(UsbError::Nak | UsbError::Nyet) => {
                stage.naks = stage.naks.saturating_add(1);
                self.stage = Some(stage);
                self.send_packet(hcd, now, registry);
            }
            Err(UsbError::Transaction) if stage.retries < PACKET_RETRIES => {
                stage.retries += 1;
                stage.errors = stage.errors.saturating_add(1);
                self.stage = Some(stage);
                self.send_packet(hcd, now, registry);
            }
            Err(error) => {
                self.stage = Some(stage);
                self.finish(hcd, Err(error), now, registry);
            }
        }
    }

    fn finish(
        &mut self,
        hcd: &mut dyn HostController,
        result: Result<usize, UsbError>,
        now: u64,
        registry: &mut Registry,
    ) {
        if let (Err(error), Some(stage)) = (result, self.stage) {
            // Everything a failure on real hardware needs in one line: what
            // was moving, how far it got, what the bus said on the way, and
            // the last channel interrupt status the controller saw.
            let snapshot = hcd.snapshot();
            let (endpoint, max_packet) = self.endpoint(stage.pipe);
            crate::usb_println!(
                "USB MSC {}: bulk {:?} ep {:#04x} ({} byte packets) {}/{} bytes failed: {:?} after {} NAK(s), {} transaction error(s); PID {:?}/{:?}, HCINT={:08x}, addr {}, {:?}",
                self.handle().index,
                stage.pipe,
                endpoint.raw(),
                max_packet,
                stage.done,
                stage.len,
                error,
                stage.naks,
                stage.errors,
                self.toggle_in,
                self.toggle_out,
                snapshot.last_interrupt,
                self.address.get(),
                self.route.device_speed
            );
        }
        self.stage = None;
        self.session.complete(result, now, registry);
    }

    /// The request to put on endpoint zero for `transfer`, and who owns it.
    pub fn control_request(&self, transfer: Transfer) -> Option<(SetupPacket, ControlOwner)> {
        let handle = self.handle();
        match transfer {
            Transfer::Control { setup } => Some((
                setup,
                ControlOwner {
                    handle,
                    clears: None,
                },
            )),
            Transfer::ClearHalt { pipe } => Some((
                wire::clear_endpoint_halt(self.endpoint(pipe).0),
                ControlOwner {
                    handle,
                    clears: Some(pipe),
                },
            )),
            _ => None,
        }
    }

    /// The manager's control pipe started the request.
    pub fn control_started(&mut self, now: u64) {
        self.session.start(now);
    }

    /// The manager's control pipe finished the request; `data` is its IN
    /// data stage.
    pub fn control_complete(
        &mut self,
        owner: ControlOwner,
        result: Result<usize, UsbError>,
        data: &[u8],
        now: u64,
        registry: &mut Registry,
    ) {
        if let Err(error) = result {
            crate::usb_println!(
                "USB MSC {}: control {:?} failed: {:?}",
                self.handle().index,
                owner
                    .clears
                    .map_or("request", |_| "CLEAR_FEATURE(ENDPOINT_HALT)"),
                error
            );
        }
        if let Ok(actual) = result {
            let target = self.session.in_buffer();
            let n = actual.min(target.len()).min(data.len());
            target[..n].copy_from_slice(&data[..n]);
            // CLEAR_FEATURE(ENDPOINT_HALT) resets the device's toggle; the
            // host's goes back with it.
            match owner.clears {
                Some(Pipe::In) => self.toggle_in = DataPid::Data0,
                Some(Pipe::Out) => self.toggle_out = DataPid::Data0,
                None => {}
            }
        }
        self.session.complete(result, now, registry);
    }
}

#[cfg(test)]
mod tests {
    use alloc::collections::VecDeque;
    use alloc::vec::Vec;

    use libusb::{EndpointDescriptor, TransferProgress, UsbSpeed};

    use super::*;
    use crate::io::fs::media::MediaId;
    use crate::io::usb::class::msc::fake_disk::{FakeDisk, pattern};
    use crate::io::usb::class::msc::registry::{Backend, DeviceInfo, MediaState, Registry};
    use crate::io::usb::hcd::{HcdSnapshot, RootPortState};

    /// What happens to the next bulk packet.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Inject {
        Nak,
        /// Lost on the way to the device: nothing changed there.
        Lost,
        /// The device took an OUT packet but its handshake was lost.
        AckLost,
        /// The device took one byte fewer than it was sent.
        ShortOut,
    }

    /// A controller in front of a [`FakeDisk`] that models what a DWC2
    /// channel leaves to software: the device's data toggles, which a packet
    /// with the wrong PID does not advance.
    struct Bus {
        disk: FakeDisk,
        inject: VecDeque<Inject>,
        device_in: DataPid,
        device_out: DataPid,
        /// Every bulk packet: direction, PID, length.
        packets: Vec<(Direction, DataPid, usize)>,
        done: VecDeque<TransferCompletion>,
        generation: u32,
        discarded: u32,
    }

    impl Bus {
        fn new(disk: FakeDisk) -> Self {
            Self {
                disk,
                inject: VecDeque::new(),
                device_in: DataPid::Data0,
                device_out: DataPid::Data0,
                packets: Vec::new(),
                done: VecDeque::new(),
                generation: 0,
                discarded: 0,
            }
        }

        fn control(&mut self, setup: SetupPacket, data: &mut [u8]) -> Result<usize, UsbError> {
            if setup.request_type == 0x02 && setup.request == 0x01 {
                if setup.index & 0x80 != 0 {
                    self.device_in = DataPid::Data0;
                } else {
                    self.device_out = DataPid::Data0;
                }
            }
            self.disk.control(setup, data)
        }

        fn packet(&mut self, request: &mut TransferRequest<'_>) -> Result<usize, UsbError> {
            let inject = self.inject.pop_front();
            match (inject, request.direction) {
                (Some(Inject::Nak), _) => return Err(UsbError::Nak),
                (Some(Inject::Lost), _) => return Err(UsbError::Transaction),
                _ => {}
            }
            match request.direction {
                Direction::Out => {
                    if request.pid != self.device_out {
                        // A repeat of a packet it already has: acknowledged
                        // and thrown away (USB 2.0 8.6.3).
                        self.discarded += 1;
                        return Ok(request.buffer.len());
                    }
                    let taken = if inject == Some(Inject::ShortOut) {
                        request.buffer.len() - 1
                    } else {
                        request.buffer.len()
                    };
                    self.device_out = self.device_out.toggled();
                    self.disk.stage_out(&request.buffer[..taken]);
                    if taken == request.buffer.len() {
                        self.disk.flush_out()?;
                    }
                    if inject == Some(Inject::AckLost) {
                        return Err(UsbError::Transaction);
                    }
                    Ok(taken)
                }
                Direction::In => {
                    assert_eq!(request.pid, self.device_in, "IN with the wrong toggle");
                    let n = self.disk.bulk_in(request.buffer)?;
                    self.device_in = self.device_in.toggled();
                    Ok(n)
                }
            }
        }
    }

    impl HostController for Bus {
        fn root_port_state(&self) -> RootPortState {
            RootPortState::Enabled(UsbSpeed::Full)
        }
        fn reset_root_port(&mut self, _: u64) -> Result<(), UsbError> {
            Ok(())
        }
        fn submit(&mut self, mut request: TransferRequest<'_>) -> Result<TransferToken, UsbError> {
            assert_eq!(request.transfer_type, TransferType::Bulk);
            assert!(
                request.buffer.len() <= request.max_packet_size as usize,
                "one packet each"
            );
            self.packets
                .push((request.direction, request.pid, request.buffer.len()));
            self.generation += 1;
            let token = TransferToken::new(0, self.generation);
            let result = self.packet(&mut request);
            self.done.push_back(TransferCompletion {
                token,
                result,
                progress: TransferProgress::Unknown,
            });
            Ok(token)
        }
        fn cancel(&mut self, _: TransferToken) -> Result<TransferProgress, UsbError> {
            Ok(TransferProgress::Unknown)
        }
        fn reap(&mut self) -> Option<TransferCompletion> {
            self.done.pop_front()
        }
        fn service_timeouts(&mut self, _: u64) {}
        fn snapshot(&self) -> HcdSnapshot {
            HcdSnapshot::default()
        }
    }

    fn endpoint(raw: u8, size: u16) -> EndpointDescriptor {
        EndpointDescriptor::parse(&[7, 5, raw, 2, size as u8, (size >> 8) as u8, 0]).unwrap()
    }

    struct Rig {
        storage: Storage,
        bus: Bus,
        registry: Registry,
        now: u64,
    }

    impl Rig {
        fn new(packet: u16) -> Self {
            let mut registry = Registry::new();
            let handle = registry
                .attach(DeviceInfo::new(Backend::Dwc2, 2, Some(1), 0))
                .unwrap();
            let interface = MscInterface {
                configuration: 1,
                number: 0,
                bulk_in: endpoint(0x81, packet),
                bulk_out: endpoint(0x02, packet),
            };
            let route = UsbRoute {
                device_speed: UsbSpeed::Full,
                translator: None,
            };
            Self {
                storage: Storage::new(UsbAddress::new(2).unwrap(), route, 64, interface, handle),
                bus: Bus::new(FakeDisk::new(512, 128)),
                registry,
                now: 0,
            }
        }

        /// One manager poll: the interface's turn, its control request if
        /// it has one, then whatever the controller finished.
        fn step(&mut self) {
            let want = self
                .storage
                .poll(&mut self.bus, self.now, &mut self.registry, true);
            if let Some(transfer) = want {
                let (setup, owner) = self.storage.control_request(transfer).unwrap();
                self.storage.control_started(self.now);
                let mut data = [0u8; 64];
                let result = self.bus.control(setup, &mut data[..setup.length as usize]);
                self.storage
                    .control_complete(owner, result, &data, self.now, &mut self.registry);
            }
            while let Some(done) = self.bus.reap() {
                self.storage
                    .complete(&mut self.bus, done, self.now, &mut self.registry);
            }
            self.now += 1_000;
        }

        fn ready(&mut self) -> MediaId {
            for _ in 0..10_000 {
                if let Some(MediaState::Ready { media_id, .. }) =
                    self.registry.state(self.storage.handle())
                {
                    return media_id;
                }
                self.step();
            }
            panic!(
                "never ready: {:?}",
                self.registry.state(self.storage.handle())
            );
        }

        fn read(
            &mut self,
            lba: u64,
            blocks: u64,
        ) -> Result<Vec<u8>, crate::io::fs::media::BlockIoError> {
            let media_id = self.ready();
            let handle = self.storage.handle();
            self.registry
                .submit_read(handle, media_id, lba, blocks, 512)?;
            for _ in 0..100_000 {
                if let Some(result) = self.registry.take_result(handle) {
                    return result;
                }
                self.step();
            }
            panic!("the read never finished");
        }
    }

    fn expected(lba: u64, blocks: u64) -> Vec<u8> {
        let mut v = Vec::new();
        for block in lba..lba + blocks {
            for offset in 0..512 {
                v.push(pattern(512, block, offset));
            }
        }
        v
    }

    #[test]
    fn a_stage_goes_one_packet_at_a_time_with_alternating_toggles() {
        let mut rig = Rig::new(8);
        assert_eq!(rig.read(3, 2).unwrap(), expected(3, 2));
        assert!(rig.bus.packets.iter().all(|&(_, _, len)| len <= 8));
        // The first CBW goes out as four packets, DATA0 DATA1 DATA0 DATA1.
        let out: Vec<_> = rig
            .bus
            .packets
            .iter()
            .filter(|p| p.0 == Direction::Out)
            .take(4)
            .map(|p| (p.1, p.2))
            .collect();
        assert_eq!(
            out,
            [
                (DataPid::Data0, 8),
                (DataPid::Data1, 8),
                (DataPid::Data0, 8),
                (DataPid::Data1, 7)
            ]
        );
        assert_eq!(rig.bus.discarded, 0);
    }

    #[test]
    fn a_nak_resends_the_same_packet_with_the_same_pid() {
        let mut rig = Rig::new(64);
        rig.ready();
        let before = rig.bus.packets.len();
        // The CBW is NAKed twice before it goes through.
        rig.bus.inject.extend([Inject::Nak, Inject::Nak]);
        assert_eq!(rig.read(0, 1).unwrap(), expected(0, 1));
        let packets = &rig.bus.packets[before..];
        assert_eq!(packets[0], packets[1]);
        assert_eq!(packets[1], packets[2]);
        assert_eq!(rig.storage.session.bot_stats().recoveries, 0);
    }

    #[test]
    fn a_lost_packet_is_resent_but_a_long_run_of_them_reaches_bot() {
        let mut rig = Rig::new(64);
        rig.ready();
        rig.bus.inject.extend([Inject::Lost; 3]);
        assert_eq!(rig.read(1, 1).unwrap(), expected(1, 1));
        assert_eq!(rig.storage.session.bot_stats().recoveries, 0);
        // A fourth in a row is BOT's to handle: Reset Recovery, then the
        // read is retried with both toggles back at DATA0.
        rig.bus.inject.extend([Inject::Lost; 4]);
        assert_eq!(rig.read(2, 1).unwrap(), expected(2, 1));
        assert_eq!(rig.storage.session.bot_stats().recoveries, 1);
    }

    #[test]
    fn a_lost_handshake_on_out_only_produces_a_duplicate_the_device_drops() {
        let mut rig = Rig::new(8);
        rig.ready();
        rig.bus.inject.push_back(Inject::AckLost);
        assert_eq!(rig.read(4, 1).unwrap(), expected(4, 1));
        assert_eq!(rig.bus.discarded, 1);
        assert_eq!(rig.storage.session.bot_stats().recoveries, 0);
    }

    #[test]
    fn a_short_out_is_not_taken_as_sent() {
        let mut rig = Rig::new(64);
        rig.ready();
        rig.bus.inject.push_back(Inject::ShortOut);
        assert_eq!(rig.read(5, 1).unwrap(), expected(5, 1));
        assert_eq!(rig.storage.session.bot_stats().recoveries, 1);
    }
}

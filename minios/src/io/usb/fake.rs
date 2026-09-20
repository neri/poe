//! A fake host controller modelling the topology of section 11.1 of
//! `docs/USB_HOST_RPI3_PLAN.md`:
//!
//! ```text
//! root port
//! `-- LAN9514 high-speed hub
//!     |-- port 1: low-speed Boot keyboard
//!     |-- port 2: full-speed Boot keyboard
//!     `-- remaining ports: empty
//! ```
//!
//! It answers the standard and class requests the manager issues, so hub
//! power/reset/debounce, transaction-translator routing, addressing and HID
//! polling can be exercised without hardware. It is deliberately strict: a
//! request it does not model is answered with a STALL rather than silently
//! succeeding.

use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell, RefMut};

use libusb::*;

use super::hcd::*;

pub const HUB_PORTS: u8 = 5;
pub const LOW_SPEED_PORT: u8 = 1;
pub const FULL_SPEED_PORT: u8 = 2;
/// `bPwrOn2PwrGood` in 2 ms units, as the LAN9514 reports it.
const POWER_ON_TO_GOOD_2MS: u8 = 50;

thread_local! {
    static CLOCK: Cell<u64> = const { Cell::new(0) };
}

/// The clock is per-thread, so tests running in parallel do not share it.
pub fn now_us() -> u64 {
    CLOCK.with(|c| c.get())
}
pub fn advance_us(us: u64) {
    CLOCK.with(|c| c.set(c.get().wrapping_add(us)))
}
pub fn reset_clock() {
    CLOCK.with(|c| c.set(0))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Submission {
    pub address: u8,
    pub endpoint: u8,
    pub transfer_type: TransferType,
    pub direction: Direction,
    pub speed: UsbSpeed,
    pub pid: DataPid,
    pub length: usize,
    /// `Some((hub address, port))` when the transfer was routed through a
    /// transaction translator.
    pub translator: Option<(u8, u8)>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Port {
    child: Option<usize>,
    powered: bool,
    enabled: bool,
    connect_change: bool,
    reset_change: bool,
}

#[derive(Clone, Debug)]
enum Kind {
    Hub { ports: Vec<Port> },
    Keyboard { report: [u8; 8], pending: bool },
}

#[derive(Clone, Debug)]
struct Device {
    speed: UsbSpeed,
    address: u8,
    configuration: u8,
    device_descriptor: Vec<u8>,
    configuration_descriptor: Vec<u8>,
    kind: Kind,
    /// Set by SET_PROTOCOL(Boot) and SET_IDLE, checked by the tests.
    pub boot_protocol: bool,
    pub idle_duration: Option<u8>,
}

fn device_descriptor(vendor: u16, product: u16, class: u8, max_packet_size_0: u8) -> Vec<u8> {
    let v = vendor.to_le_bytes();
    let p = product.to_le_bytes();
    vec![
        18,
        1,
        0x00,
        0x02,
        class,
        0,
        0,
        max_packet_size_0,
        v[0],
        v[1],
        p[0],
        p[1],
        0x00,
        0x01,
        0,
        0,
        0,
        1,
    ]
}

fn hub_configuration() -> Vec<u8> {
    let mut b = vec![
        9, 2, 25, 0, 1, 1, 0, 0xe0, 1, // configuration
        9, 4, 0, 0, 1, 9, 0, 0, 0, // interface, class 9
        7, 5, 0x81, 3, 1, 0, 12, // status change endpoint
    ];
    let total = b.len() as u16;
    b[2..4].copy_from_slice(&total.to_le_bytes());
    b
}

fn keyboard_configuration() -> Vec<u8> {
    let mut b = vec![
        9, 2, 34, 0, 1, 1, 0, 0x80, 50, // configuration
        9, 4, 0, 0, 1, 3, 1, 1, 0, // interface, HID boot keyboard
        9, 0x21, 0x11, 0x01, 0, 1, 0x22, 65, 0, // HID descriptor
        7, 5, 0x81, 3, 8, 0, 10, // interrupt IN, 8 bytes, 10 ms
    ];
    let total = b.len() as u16;
    b[2..4].copy_from_slice(&total.to_le_bytes());
    b
}

pub struct Bus {
    devices: Vec<Device>,
    /// Index of the device attached to the root port.
    root: Option<usize>,
    root_enabled: bool,
    root_reset_deadline: Option<u64>,
    /// The device that answers at address zero, set when a reset completes.
    default_address: Option<usize>,
    setup: Vec<Option<SetupPacket>>,
    /// How far into the current control data stage each device has got. A
    /// transfer through a transaction translator is issued one packet at a
    /// time, so the device has to remember where it left off.
    data_offset: Vec<usize>,
    completions: Vec<TransferCompletion>,
    /// Interrupt transfers do not finish in the same instant they are
    /// submitted; holding them for a frame is what lets a disconnect find one
    /// outstanding and cancel it.
    deferred: Vec<(TransferCompletion, u64)>,
    /// When set, interrupt transfers never finish, so one is always
    /// outstanding.
    hold_interrupts: bool,
    generation: u32,
    snapshot: HcdSnapshot,
    pub submissions: Vec<Submission>,
    pub cancelled: Vec<TransferToken>,
    /// Requests the model does not implement, for the tests to assert on.
    pub stalled: Vec<SetupPacket>,
}

impl Bus {
    /// The LAN9514 with a low-speed keyboard on port 1 and a full-speed one
    /// on port 2, nothing else attached.
    pub fn pi3_topology() -> Self {
        reset_clock();
        let hub = Device {
            speed: UsbSpeed::High,
            address: 0,
            configuration: 0,
            device_descriptor: device_descriptor(0x0424, 0x9514, 9, 64),
            configuration_descriptor: hub_configuration(),
            kind: Kind::Hub {
                ports: vec![Port::default(); HUB_PORTS as usize],
            },
            boot_protocol: false,
            idle_duration: None,
        };
        let keyboard = |vendor, product, speed| Device {
            speed,
            address: 0,
            configuration: 0,
            device_descriptor: device_descriptor(vendor, product, 0, 8),
            configuration_descriptor: keyboard_configuration(),
            kind: Kind::Keyboard {
                report: [0; 8],
                pending: false,
            },
            boot_protocol: false,
            idle_duration: None,
        };
        let mut this = Self {
            devices: vec![
                hub,
                keyboard(0x1c4f, 0x0027, UsbSpeed::Low),
                keyboard(0x046d, 0xc31c, UsbSpeed::Full),
            ],
            root: Some(0),
            root_enabled: false,
            root_reset_deadline: None,
            default_address: None,
            setup: vec![None; 3],
            data_offset: vec![0; 3],
            completions: Vec::new(),
            deferred: Vec::new(),
            hold_interrupts: false,
            generation: 0,
            snapshot: HcdSnapshot::default(),
            submissions: Vec::new(),
            cancelled: Vec::new(),
            stalled: Vec::new(),
        };
        this.plug(LOW_SPEED_PORT, 1);
        this.plug(FULL_SPEED_PORT, 2);
        this
    }

    fn plug(&mut self, port: u8, device: usize) {
        if let Kind::Hub { ports } = &mut self.devices[0].kind
            && let Some(slot) = ports.get_mut(port as usize - 1)
        {
            slot.child = Some(device);
            slot.connect_change = true;
        }
    }

    /// Removes the whole tree from the root port, as pulling the cable would.
    pub fn unplug_root(&mut self) {
        self.root = None;
        self.root_enabled = false;
        self.default_address = None;
        for device in &mut self.devices {
            device.address = 0;
            device.configuration = 0;
        }
    }

    /// Removes whatever is on a downstream port, as a disconnection would.
    pub fn unplug(&mut self, port: u8) {
        let mut removed = None;
        if let Kind::Hub { ports } = &mut self.devices[0].kind
            && let Some(slot) = ports.get_mut(port as usize - 1)
        {
            removed = slot.child.take();
            slot.enabled = false;
            slot.connect_change = true;
        }
        if let Some(child) = removed {
            self.devices[child].address = 0;
            self.devices[child].configuration = 0;
        }
    }

    /// Makes the next interrupt-IN poll of that device return a report.
    pub fn press_key(&mut self, port: u8, modifiers: u8, keys: &[u8]) {
        let Some(index) = self.child_of(port) else {
            return;
        };
        if let Kind::Keyboard { report, pending } = &mut self.devices[index].kind {
            *report = [0; 8];
            report[0] = modifiers;
            for (slot, key) in report[2..].iter_mut().zip(keys) {
                *slot = *key;
            }
            *pending = true;
        }
    }

    fn child_of(&self, port: u8) -> Option<usize> {
        match &self.devices[0].kind {
            Kind::Hub { ports } => ports.get(port as usize - 1)?.child,
            _ => None,
        }
    }

    /// The address the manager assigned to the device on that port, if any.
    pub fn assigned_address(&self, port: u8) -> Option<u8> {
        let index = self.child_of(port)?;
        (self.devices[index].address != 0).then_some(self.devices[index].address)
    }

    pub fn is_configured(&self, port: u8) -> bool {
        self.child_of(port)
            .is_some_and(|index| self.devices[index].configuration != 0)
    }

    pub fn boot_protocol_set(&self, port: u8) -> bool {
        self.child_of(port)
            .is_some_and(|index| self.devices[index].boot_protocol)
    }

    /// The idle duration the manager asked for, in 4 ms units.
    pub fn idle_duration(&self, port: u8) -> Option<u8> {
        self.child_of(port)
            .and_then(|index| self.devices[index].idle_duration)
    }

    fn target(&self, address: u8) -> Option<usize> {
        if address == 0 {
            return self.default_address;
        }
        self.devices
            .iter()
            .position(|d| d.address == address && self.reachable(d))
    }

    fn reachable(&self, device: &Device) -> bool {
        match &self.devices[0].kind {
            Kind::Hub { ports } => {
                core::ptr::eq(device, &self.devices[0])
                    || ports.iter().any(|p| {
                        p.child
                            .is_some_and(|c| core::ptr::eq(&self.devices[c], device))
                    })
            }
            _ => true,
        }
    }

    fn port_status(&self, port: u8) -> (u16, u16) {
        let Kind::Hub { ports } = &self.devices[0].kind else {
            return (0, 0);
        };
        let Some(slot) = ports.get(port as usize - 1) else {
            return (0, 0);
        };
        let mut status = 0u16;
        if slot.powered {
            status |= 1 << 8;
        }
        if let Some(child) = slot.child {
            status |= 1;
            if slot.enabled {
                status |= 1 << 1;
                match self.devices[child].speed {
                    UsbSpeed::Low => status |= 1 << 9,
                    UsbSpeed::High => status |= 1 << 10,
                    UsbSpeed::Full => {}
                }
            }
        }
        let mut change = 0u16;
        if slot.connect_change {
            change |= 1;
        }
        if slot.reset_change {
            change |= 1 << 4;
        }
        (status, change)
    }

    fn control_in(&mut self, index: usize, setup: SetupPacket, out: &mut [u8]) -> Option<usize> {
        let device = &self.devices[index];
        let bytes: Vec<u8> = match (setup.request_type, setup.request) {
            (0x80, 6) => match setup.value >> 8 {
                1 => device.device_descriptor.clone(),
                2 => device.configuration_descriptor.clone(),
                _ => return None,
            },
            (0xa0, 6) if (setup.value >> 8) as u8 == DESCRIPTOR_HUB => {
                let Kind::Hub { ports } = &device.kind else {
                    return None;
                };
                vec![
                    9,
                    DESCRIPTOR_HUB,
                    ports.len() as u8,
                    0x09,
                    0x00,
                    POWER_ON_TO_GOOD_2MS,
                    10,
                    0x00,
                    0xff,
                ]
            }
            (0xa3, 0) => {
                let (status, change) = self.port_status(setup.index as u8);
                let mut bytes = Vec::new();
                bytes.extend_from_slice(&status.to_le_bytes());
                bytes.extend_from_slice(&change.to_le_bytes());
                bytes
            }
            _ => return None,
        };
        let offset = self.data_offset[index].min(bytes.len());
        let length = (bytes.len() - offset).min(out.len());
        out[..length].copy_from_slice(&bytes[offset..offset + length]);
        self.data_offset[index] = offset + length;
        Some(length)
    }

    /// Applies the effect of a control OUT at its status stage, as a device
    /// does.
    fn control_out(&mut self, index: usize, setup: SetupPacket) -> bool {
        match (setup.request_type, setup.request) {
            (0x00, 5) => {
                self.devices[index].address = setup.value as u8;
                self.default_address = None;
            }
            (0x00, 9) => self.devices[index].configuration = setup.value as u8,
            (0x21, 0x0a) => self.devices[index].idle_duration = Some((setup.value >> 8) as u8),
            (0x21, 0x0b) => self.devices[index].boot_protocol = setup.value == 0,
            (0x23, 3) => return self.set_port_feature(setup.value, setup.index as u8),
            (0x23, 1) => return self.clear_port_feature(setup.value, setup.index as u8),
            // CLEAR_TT_BUFFER.
            (0x23, 8) => {}
            _ => return false,
        }
        true
    }

    fn set_port_feature(&mut self, feature: u16, port: u8) -> bool {
        let Kind::Hub { ports } = &mut self.devices[0].kind else {
            return false;
        };
        let Some(slot) = ports.get_mut(port as usize - 1) else {
            return false;
        };
        match feature {
            // PORT_POWER.
            8 => slot.powered = true,
            // PORT_RESET. Hubs complete the reset on their own; the manager
            // polls GET_STATUS until the reset bit clears.
            4 => {
                slot.reset_change = true;
                let child = slot.child;
                if let Some(child) = child {
                    slot.enabled = true;
                    self.default_address = Some(child);
                    self.devices[child].address = 0;
                }
            }
            _ => return false,
        }
        true
    }

    fn clear_port_feature(&mut self, feature: u16, port: u8) -> bool {
        let Kind::Hub { ports } = &mut self.devices[0].kind else {
            return false;
        };
        let Some(slot) = ports.get_mut(port as usize - 1) else {
            return false;
        };
        match feature {
            // C_PORT_CONNECTION.
            16 => slot.connect_change = false,
            // C_PORT_RESET.
            20 => slot.reset_change = false,
            _ => return false,
        }
        true
    }

    fn completion(token: TransferToken, result: Result<usize, UsbError>) -> TransferCompletion {
        let progress = match result {
            Ok(actual) => TransferProgress::Known(actual),
            Err(UsbError::Nak) => TransferProgress::Known(0),
            Err(_) => TransferProgress::Unknown,
        };
        TransferCompletion {
            token,
            result,
            progress,
        }
    }

    fn complete(&mut self, token: TransferToken, result: Result<usize, UsbError>) {
        self.snapshot.active = self.snapshot.active.saturating_sub(1);
        self.completions.push(Self::completion(token, result))
    }

    /// Finishes a transfer one frame from now, leaving it outstanding until
    /// then.
    fn complete_later(&mut self, token: TransferToken, result: Result<usize, UsbError>) {
        let due = if self.hold_interrupts {
            u64::MAX
        } else {
            now_us().saturating_add(1_000)
        };
        self.deferred.push((Self::completion(token, result), due))
    }

    /// Stops interrupt transfers from ever finishing, so the next one submitted
    /// stays outstanding.
    pub fn hold_interrupts(&mut self) {
        self.hold_interrupts = true
    }

    /// True while a transfer the manager submitted has not been answered.
    pub fn has_outstanding_transfer(&self) -> bool {
        !self.deferred.is_empty()
    }
}

/// A handle the test and the manager share, so the bus can be inspected and
/// driven while `UsbManager` owns its `Box<dyn HostController>`.
#[derive(Clone)]
pub struct FakeHcd(Rc<RefCell<Bus>>);

impl FakeHcd {
    pub fn pi3_topology() -> Self {
        Self(Rc::new(RefCell::new(Bus::pi3_topology())))
    }
    pub fn bus(&self) -> RefMut<'_, Bus> {
        self.0.borrow_mut()
    }
}

impl HostController for FakeHcd {
    fn root_port_state(&self) -> RootPortState {
        self.0.borrow().root_port_state()
    }
    fn reset_root_port(&mut self, deadline_us: u64) -> Result<(), UsbError> {
        self.0.borrow_mut().reset_root_port(deadline_us)
    }
    fn submit(&mut self, request: TransferRequest<'_>) -> Result<TransferToken, UsbError> {
        self.0.borrow_mut().submit(request)
    }
    fn cancel(&mut self, token: TransferToken) -> Result<TransferProgress, UsbError> {
        self.0.borrow_mut().cancel(token)
    }
    fn reap(&mut self) -> Option<TransferCompletion> {
        self.0.borrow_mut().reap()
    }
    fn service_timeouts(&mut self, now_us: u64) {
        self.0.borrow_mut().service_timeouts(now_us)
    }
    fn snapshot(&self) -> HcdSnapshot {
        self.0.borrow().snapshot()
    }
}

impl HostController for Bus {
    fn root_port_state(&self) -> RootPortState {
        match self.root {
            None => RootPortState::Disconnected,
            Some(index) if self.root_enabled => RootPortState::Enabled(self.devices[index].speed),
            Some(index) => RootPortState::Connected(self.devices[index].speed),
        }
    }

    fn reset_root_port(&mut self, deadline_us: u64) -> Result<(), UsbError> {
        self.root_enabled = false;
        self.root_reset_deadline = Some(deadline_us);
        Ok(())
    }

    fn submit(&mut self, request: TransferRequest<'_>) -> Result<TransferToken, UsbError> {
        self.generation = self.generation.wrapping_add(1).max(1);
        let token = TransferToken::new(0, self.generation);
        self.snapshot.submitted = self.snapshot.submitted.saturating_add(1);
        self.snapshot.active = self.snapshot.active.saturating_add(1);
        self.submissions.push(Submission {
            address: request.address.get(),
            endpoint: request.endpoint.raw(),
            transfer_type: request.transfer_type,
            direction: request.direction,
            speed: request.route.device_speed,
            pid: request.pid,
            length: request.buffer.len(),
            translator: request
                .route
                .translator
                .map(|t| (t.hub_address.get(), t.port_number)),
        });

        let Some(index) = self.target(request.address.get()) else {
            self.complete(token, Err(UsbError::Timeout));
            return Ok(token);
        };

        match request.transfer_type {
            TransferType::Control => {
                if request.pid == DataPid::Setup {
                    let mut bytes = [0u8; 8];
                    bytes.copy_from_slice(&request.buffer[..8]);
                    self.setup[index] = Some(SetupPacket::new(
                        bytes[0],
                        bytes[1],
                        u16::from_le_bytes([bytes[2], bytes[3]]),
                        u16::from_le_bytes([bytes[4], bytes[5]]),
                        u16::from_le_bytes([bytes[6], bytes[7]]),
                    ));
                    self.data_offset[index] = 0;
                    self.complete(token, Ok(8));
                    return Ok(token);
                }
                let Some(setup) = self.setup[index] else {
                    self.complete(token, Err(UsbError::Stall));
                    return Ok(token);
                };
                if request.buffer.is_empty() {
                    // Status stage: this is where a device applies the effect
                    // of a control OUT.
                    let accepted =
                        setup.direction() == Direction::In || self.control_out(index, setup);
                    if accepted {
                        self.complete(token, Ok(0));
                    } else {
                        self.stalled.push(setup);
                        self.complete(token, Err(UsbError::Stall));
                    }
                    return Ok(token);
                }
                match self.control_in(index, setup, request.buffer) {
                    Some(actual) => self.complete(token, Ok(actual)),
                    None => {
                        self.stalled.push(setup);
                        self.complete(token, Err(UsbError::Stall))
                    }
                }
            }
            TransferType::Interrupt => {
                let report = match &mut self.devices[index].kind {
                    Kind::Keyboard { report, pending } if *pending => {
                        *pending = false;
                        Some(*report)
                    }
                    _ => None,
                };
                match report {
                    Some(report) => {
                        let length = request.buffer.len().min(report.len());
                        request.buffer[..length].copy_from_slice(&report[..length]);
                        self.complete_later(token, Ok(length))
                    }
                    None => self.complete_later(token, Err(UsbError::Nak)),
                }
            }
            _ => self.complete(token, Err(UsbError::Unsupported)),
        }
        Ok(token)
    }

    fn cancel(&mut self, token: TransferToken) -> Result<TransferProgress, UsbError> {
        self.cancelled.push(token);
        self.snapshot.cancelled = self.snapshot.cancelled.saturating_add(1);
        self.snapshot.active = self.snapshot.active.saturating_sub(1);
        self.completions.retain(|c| c.token != token);
        self.deferred.retain(|(c, _)| c.token != token);
        Ok(TransferProgress::Unknown)
    }

    fn reap(&mut self) -> Option<TransferCompletion> {
        let completion = self.completions.first().copied()?;
        self.completions.remove(0);
        self.snapshot.reaped = self.snapshot.reaped.saturating_add(1);
        Some(completion)
    }

    fn service_timeouts(&mut self, now_us: u64) {
        let mut index = 0;
        while index < self.deferred.len() {
            if self.deferred[index].1 <= now_us {
                let (completion, _) = self.deferred.remove(index);
                self.snapshot.active = self.snapshot.active.saturating_sub(1);
                self.completions.push(completion);
            } else {
                index += 1;
            }
        }
        if let Some(deadline) = self.root_reset_deadline
            && now_us >= deadline
        {
            self.root_reset_deadline = None;
            self.root_enabled = true;
            if let Some(index) = self.root {
                self.devices[index].address = 0;
                self.default_address = Some(index);
            }
        }
    }

    fn snapshot(&self) -> HcdSnapshot {
        self.snapshot
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;

    use super::*;
    use crate::env::SystemService;
    use crate::io::usb::UsbManager;
    use crate::io::usb::manager::DeviceLocation;

    /// Polls the manager on a 1 ms tick until `done`, for at most 20 seconds
    /// of modelled time.
    fn run_until(
        manager: &mut UsbManager,
        bus: &FakeHcd,
        mut done: impl FnMut(&UsbManager, &Bus) -> bool,
    ) -> bool {
        for _ in 0..20_000 {
            manager.poll().unwrap();
            if done(manager, &bus.bus()) {
                return true;
            }
            advance_us(1_000);
        }
        false
    }

    /// Brings up the bus with a keyboard on `port` only, and waits until the
    /// manager has bound it as the console keyboard. The manager owns one
    /// keyboard at a time, so the other port is left empty.
    fn with_keyboard_on(port: u8) -> (UsbManager, FakeHcd) {
        let bus = FakeHcd::pi3_topology();
        let other = if port == LOW_SPEED_PORT {
            FULL_SPEED_PORT
        } else {
            LOW_SPEED_PORT
        };
        bus.bus().unplug(other);
        let mut manager = UsbManager::new(Box::new(bus.clone()), now_us);
        assert!(
            run_until(&mut manager, &bus, |manager, _| manager.keyboard_ready()),
            "the keyboard on port {port} was never bound; devices: {:?}",
            manager.devices()
        );
        (manager, bus)
    }

    #[test]
    fn enumerates_the_hub_and_a_low_speed_keyboard() {
        let (manager, bus) = with_keyboard_on(LOW_SPEED_PORT);
        assert_eq!(bus.bus().assigned_address(LOW_SPEED_PORT), Some(2));
        assert!(bus.bus().is_configured(LOW_SPEED_PORT));
        let devices = manager.devices();
        assert_eq!(devices.len(), 2, "the hub and the keyboard");
        assert_eq!(devices[0].address.get(), 1);
        assert_eq!(devices[0].speed, UsbSpeed::High);
        assert_eq!(devices[1].speed, UsbSpeed::Low);
        assert_eq!(devices[1].vendor_id, Some(0x1c4f));
        assert_eq!(devices[1].product_id, Some(0x0027));
        assert_eq!(devices[1].configuration, Some(1));
        assert_eq!(
            devices[1].location,
            DeviceLocation::Hub {
                hub: UsbAddress::new(1).unwrap(),
                port: LOW_SPEED_PORT
            }
        );
    }

    #[test]
    fn enumerates_a_full_speed_keyboard_on_another_port() {
        let (manager, bus) = with_keyboard_on(FULL_SPEED_PORT);
        assert!(bus.bus().is_configured(FULL_SPEED_PORT));
        let devices = manager.devices();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[1].speed, UsbSpeed::Full);
        assert_eq!(devices[1].vendor_id, Some(0x046d));
    }

    #[test]
    fn no_request_outside_the_model_is_issued() {
        for port in [LOW_SPEED_PORT, FULL_SPEED_PORT] {
            let (_manager, bus) = with_keyboard_on(port);
            let stalled = bus.bus().stalled.clone();
            assert!(
                stalled.is_empty(),
                "port {port}: the manager issued requests the device model does not implement: {stalled:?}"
            );
        }
    }

    #[test]
    fn children_of_a_high_speed_hub_are_routed_through_its_translator() {
        for (port, speed) in [
            (LOW_SPEED_PORT, UsbSpeed::Low),
            (FULL_SPEED_PORT, UsbSpeed::Full),
        ] {
            let (_manager, bus) = with_keyboard_on(port);
            let bus = bus.bus();
            let child = bus.assigned_address(port).unwrap();
            let mut seen = 0;
            for submission in bus.submissions.iter() {
                if submission.address == 1 {
                    assert_eq!(
                        submission.translator, None,
                        "the high-speed hub itself must be addressed directly"
                    );
                } else if submission.address == child {
                    assert_eq!(
                        submission.translator,
                        Some((1, port)),
                        "a {speed:?}-speed child must be routed through the hub's translator"
                    );
                    assert_eq!(submission.speed, speed);
                    seen += 1;
                }
            }
            assert!(seen > 0, "port {port}: no transfer reached the child");
        }
    }

    #[test]
    fn address_zero_traffic_is_also_split_routed() {
        // Before it has an address, a child is still behind the translator.
        let (_manager, bus) = with_keyboard_on(LOW_SPEED_PORT);
        let bus = bus.bus();
        assert!(
            bus.submissions
                .iter()
                .any(|s| s.address == 0 && s.translator == Some((1, LOW_SPEED_PORT))),
            "the default-address transfers must carry the translator too"
        );
    }

    #[test]
    fn every_port_is_powered_before_any_of_them_is_scanned() {
        let (_manager, bus) = with_keyboard_on(LOW_SPEED_PORT);
        let bus = bus.bus();
        // A port status read is the only 4-byte IN the manager makes of the
        // hub; SET_FEATURE(PORT_POWER) is a zero-length OUT before it.
        let first_status = bus
            .submissions
            .iter()
            .position(|s| s.address == 1 && s.direction == Direction::In && s.length == 4)
            .expect("the manager should read port status");
        let setups = bus.submissions[..first_status]
            .iter()
            .filter(|s| s.address == 1 && s.pid == DataPid::Setup)
            .count();
        assert!(
            setups >= HUB_PORTS as usize,
            "expected all {HUB_PORTS} ports powered before the first status read, \
             only {setups} control requests preceded it"
        );
    }

    #[test]
    fn the_boot_protocol_and_idle_are_set_on_the_bound_keyboard() {
        for port in [LOW_SPEED_PORT, FULL_SPEED_PORT] {
            let (_manager, bus) = with_keyboard_on(port);
            assert!(
                bus.bus().boot_protocol_set(port),
                "port {port}: SET_PROTOCOL"
            );
            // A duration of zero would make every dropped report a dropped
            // key, and it has to be shorter than the 10 ms poll interval for
            // the device to restate itself in time.
            let duration = bus.bus().idle_duration(port);
            assert!(duration.is_some(), "port {port}: SET_IDLE was not sent");
            let duration = duration.unwrap();
            assert!(
                duration > 0,
                "port {port}: SET_IDLE asked for report-on-change only"
            );
            assert!(
                (duration as u32) * 4 < 10,
                "port {port}: idle duration {} ms is not shorter than the poll interval",
                duration as u32 * 4
            );
        }
    }

    #[test]
    fn the_keyboard_is_polled_at_its_declared_interval() {
        let (mut manager, bus) = with_keyboard_on(LOW_SPEED_PORT);
        let child = bus.bus().assigned_address(LOW_SPEED_PORT).unwrap();
        let before = bus
            .bus()
            .submissions
            .iter()
            .filter(|s| s.address == child && s.transfer_type == TransferType::Interrupt)
            .count();
        let start = now_us();
        while now_us() - start < 1_000_000 {
            manager.poll().unwrap();
            advance_us(1_000);
        }
        let polls = bus
            .bus()
            .submissions
            .iter()
            .filter(|s| s.address == child && s.transfer_type == TransferType::Interrupt)
            .count()
            - before;
        // The endpoint declares 10 ms, so one second of ticks is about 100
        // polls. A deadline computed from the completion time instead of the
        // previous target halves this.
        assert!(
            (90..=110).contains(&polls),
            "expected about 100 polls in a second at a 10 ms interval, got {polls}"
        );
    }

    #[test]
    fn an_idle_keyboard_does_not_look_like_a_fault() {
        let (mut manager, bus) = with_keyboard_on(LOW_SPEED_PORT);
        let errors = manager.snapshot().errors;
        let start = now_us();
        while now_us() - start < 2_000_000 {
            manager.poll().unwrap();
            advance_us(1_000);
        }
        let polls = bus
            .bus()
            .submissions
            .iter()
            .filter(|s| s.transfer_type == TransferType::Interrupt)
            .count();
        assert!(polls > 100, "the keyboard should have been polled");
        assert_eq!(
            manager.snapshot().errors,
            errors,
            "a NAK from an idle keyboard is the normal answer, not an error"
        );
    }

    #[test]
    fn a_key_press_reaches_the_console_queue() {
        while crate::io::usb::input::take().is_some() {}
        let (mut manager, bus) = with_keyboard_on(LOW_SPEED_PORT);
        // 0x04 is the HID usage for 'a'.
        bus.bus().press_key(LOW_SPEED_PORT, 0, &[0x04]);
        assert!(
            run_until(&mut manager, &bus, |_, _| {
                crate::io::usb::input::take().is_some()
            }),
            "the press never reached the console queue"
        );
    }

    #[test]
    fn a_removed_keyboard_is_detached_and_never_addressed_again() {
        let (mut manager, bus) = with_keyboard_on(LOW_SPEED_PORT);
        let address = bus.bus().assigned_address(LOW_SPEED_PORT).unwrap();
        let topology = manager.snapshot().topology_generation;
        bus.bus().unplug(LOW_SPEED_PORT);
        assert!(
            run_until(&mut manager, &bus, |manager, _| {
                !manager.devices().iter().any(|d| d.address.get() == address)
            }),
            "the removed keyboard is still in the device list"
        );
        assert!(
            manager.snapshot().topology_generation > topology,
            "removal must advance the topology generation"
        );
        assert!(
            !manager.keyboard_ready(),
            "the console keyboard must not survive its device"
        );
        let mark = bus.bus().submissions.len();
        let start = now_us();
        while now_us() - start < 2_000_000 {
            manager.poll().unwrap();
            advance_us(1_000);
        }
        let after: Vec<_> = bus.bus().submissions[mark..]
            .iter()
            .filter(|s| s.address == address)
            .copied()
            .collect();
        assert!(
            after.is_empty(),
            "the manager kept talking to the removed device: {after:?}"
        );
    }

    #[test]
    fn a_root_disconnect_cancels_the_outstanding_poll_and_detaches_everything() {
        let (mut manager, bus) = with_keyboard_on(LOW_SPEED_PORT);
        // Leave a keyboard poll outstanding, so the disconnect has something
        // to cancel rather than finding the session idle between polls.
        bus.bus().hold_interrupts();
        assert!(
            run_until(&mut manager, &bus, |_, bus| bus.has_outstanding_transfer()),
            "no keyboard poll was left outstanding"
        );
        bus.bus().unplug_root();
        manager.poll().unwrap();
        assert!(
            !bus.bus().cancelled.is_empty(),
            "the outstanding poll should have been cancelled"
        );
        assert!(manager.devices().is_empty(), "every device should be gone");
        assert!(!manager.keyboard_ready());
        assert_eq!(manager.snapshot().disconnects, 2);
    }

    #[test]
    fn the_hub_survives_a_child_disconnect() {
        let (mut manager, bus) = with_keyboard_on(LOW_SPEED_PORT);
        bus.bus().unplug(LOW_SPEED_PORT);
        assert!(run_until(&mut manager, &bus, |manager, _| {
            manager.devices().len() == 1
        }));
        assert_eq!(manager.devices()[0].address.get(), 1, "the hub stays");
        assert_eq!(manager.devices()[0].configuration, Some(1));
    }
}

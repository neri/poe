use alloc::boxed::Box;
use alloc::vec::Vec;

use libusb::{
    ConfigurationDescriptor, DESCRIPTOR_CONFIGURATION, DESCRIPTOR_DEVICE, DESCRIPTOR_HUB, DataPid,
    DescriptorIter, DeviceDescriptor, Direction, EndpointDescriptor, HubDescriptor,
    InterfaceDescriptor, SetupPacket, TransferType, UsbAddress, UsbError, UsbSpeed,
};

use super::class::hid::{IDLE_DURATION_4MS, set_idle, set_protocol_boot};
use super::class::hid_keyboard::{BOOT_REPORT_SIZE, BootKeyboard};
use super::class::hub::{HubPort, MAX_DOWNSTREAM_PORTS};
use super::control::{ControlRequestContext, ControlTransfer};
use super::hcd::{
    HostController, RootPortState, SplitTarget, TransferCompletion, TransferRequest, TransferToken,
    UsbRoute,
};
use crate::env::{ServiceError, SystemService};

pub const MAX_DEVICES: usize = 16;
pub const MAX_CONFIGURATION_DESCRIPTOR: usize = 512;
pub const COMPLETION_BUDGET: usize = 32;
const HUB_MONITOR_INTERVAL_US: u64 = 1_000_000;
/// How often the keyboard session reports its counters, with `usb_debug` on.
const HID_STATS_INTERVAL_US: u64 = 10_000_000;
const TT_RECOVERY_DELAY_US: u64 = 2_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceLocation {
    Root,
    Hub { hub: UsbAddress, port: u8 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceRecord {
    pub address: UsbAddress,
    pub generation: u32,
    pub speed: UsbSpeed,
    pub location: DeviceLocation,
    pub configuration: Option<u8>,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub descriptor: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ManagerSnapshot {
    pub topology_generation: u32,
    pub device_count: u8,
    pub completions: u64,
    pub disconnects: u64,
    pub errors: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RootScan {
    Idle,
    Debouncing { until_us: u64 },
    Resetting { until_us: u64 },
    Ready,
    ReadingFirstDescriptor,
    SettingAddress { address: UsbAddress },
    AddressDelay { address: UsbAddress, until_us: u64 },
    ReadingDevice { address: UsbAddress },
    ReadingConfigHeader { address: UsbAddress },
    ReadingConfig { address: UsbAddress },
    SettingConfiguration { address: UsbAddress, value: u8 },
    Configured,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HubScan {
    Idle,
    ReadingDescriptor {
        hub: UsbAddress,
    },
    PoweringPorts {
        hub: UsbAddress,
        port: u8,
        ports: u8,
        power_good_us: u64,
    },
    WaitingForPower {
        hub: UsbAddress,
        ports: u8,
        until_us: u64,
    },
    ReadingPortStatus {
        hub: UsbAddress,
        port: u8,
        ports: u8,
    },
    /// A port scan that stood aside for a keyboard poll, with the last port
    /// it finished.
    ScanPaused {
        hub: UsbAddress,
        port: u8,
        ports: u8,
    },
    ResettingPort {
        hub: UsbAddress,
        port: u8,
        ports: u8,
    },
    WaitingForReset {
        hub: UsbAddress,
        port: u8,
        ports: u8,
        until_us: u64,
    },
    ReadingResetStatus {
        hub: UsbAddress,
        port: u8,
        ports: u8,
    },
    ClearingConnectionChange {
        hub: UsbAddress,
        port: u8,
        ports: u8,
    },
    ClearingResetChange {
        hub: UsbAddress,
        port: u8,
        ports: u8,
        speed: UsbSpeed,
    },
    Monitoring {
        hub: UsbAddress,
        ports: u8,
        next_scan_us: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChildScan {
    Idle,
    ResetRecovery {
        hub: UsbAddress,
        port: u8,
        speed: UsbSpeed,
        until_us: u64,
    },
    ReadingFirst {
        hub: UsbAddress,
        port: u8,
        speed: UsbSpeed,
    },
    SettingAddress {
        hub: UsbAddress,
        port: u8,
        speed: UsbSpeed,
        address: UsbAddress,
    },
    AddressDelay {
        hub: UsbAddress,
        port: u8,
        speed: UsbSpeed,
        address: UsbAddress,
        until_us: u64,
    },
    ReadingDevice {
        hub: UsbAddress,
        port: u8,
        speed: UsbSpeed,
        address: UsbAddress,
    },
    ReadingConfigHeader {
        hub: UsbAddress,
        port: u8,
        speed: UsbSpeed,
        address: UsbAddress,
    },
    ReadingConfig {
        hub: UsbAddress,
        port: u8,
        speed: UsbSpeed,
        address: UsbAddress,
    },
    SettingConfiguration {
        hub: UsbAddress,
        port: u8,
        speed: UsbSpeed,
        address: UsbAddress,
        value: u8,
    },
    SettingProtocol {
        address: UsbAddress,
        route: UsbRoute,
        interface: u8,
        endpoint: EndpointDescriptor,
    },
    SettingIdle {
        address: UsbAddress,
        route: UsbRoute,
        endpoint: EndpointDescriptor,
    },
    Running,
}

#[derive(Clone, Copy)]
struct TtRecovery {
    request: ControlRequestContext,
    hub: UsbAddress,
    port: u8,
    devinfo: u16,
    clearing_second_direction: bool,
    retry_at_us: Option<u64>,
}

struct HidSession {
    address: UsbAddress,
    route: UsbRoute,
    endpoint: EndpointDescriptor,
    report: [u8; 64],
    token: Option<TransferToken>,
    next_poll_us: u64,
    keyboard: BootKeyboard,
    pid: DataPid,
    consecutive_errors: u8,
    /// Diagnostic counts for this session, reported when it is torn down.
    polls: u64,
    reports: u64,
    naks: u64,
    errors: u64,
    next_stats_us: u64,
}

struct AddressAllocator {
    used: [bool; 128],
    next: u8,
}
impl AddressAllocator {
    fn new() -> Self {
        let mut used = [false; 128];
        used[0] = true;
        Self { used, next: 1 }
    }
    fn allocate(&mut self) -> Result<UsbAddress, UsbError> {
        for offset in 0..127u8 {
            let raw = 1 + ((self.next - 1 + offset) % 127);
            if !self.used[raw as usize] {
                self.used[raw as usize] = true;
                self.next = if raw == 127 { 1 } else { raw + 1 };
                return Ok(UsbAddress::new(raw).unwrap());
            }
        }
        Err(UsbError::ResourceExhausted)
    }
    fn release(&mut self, address: UsbAddress) {
        if address.get() != 0 {
            self.used[address.get() as usize] = false
        }
    }
}

/// The single owner of a USB bus. Protocol/class layers receive completions
/// from this object and never access controller channels directly.
pub struct UsbManager {
    hcd: Box<dyn HostController>,
    addresses: AddressAllocator,
    devices: Vec<DeviceRecord>,
    next_device_generation: u32,
    snapshot: ManagerSnapshot,
    now_us: fn() -> u64,
    last_root_state: RootPortState,
    completions: Vec<TransferCompletion>,
    root_scan: RootScan,
    root_speed: UsbSpeed,
    ep0_packet_size: u16,
    control: ControlTransfer,
    hub_scan: HubScan,
    hub_ports: Vec<HubPort>,
    child_scan: ChildScan,
    child_ep0: u16,
    tt_recovery: Option<TtRecovery>,
    tt_recovery_attempts: u8,
    /// Keyboard polls skipped because a control transfer was in flight.
    hid_blocked: u64,
    hid: Option<HidSession>,
    last_reset_status: Option<(UsbAddress, u8, u16, u16)>,
    last_reset_error: Option<(UsbAddress, u8, u32)>,
    fallback_probe: Option<(UsbAddress, u8, UsbSpeed)>,
}

impl UsbManager {
    pub fn new(hcd: Box<dyn HostController>, now_us: fn() -> u64) -> Self {
        let last_root_state = hcd.root_port_state();
        Self {
            hcd,
            addresses: AddressAllocator::new(),
            devices: Vec::new(),
            next_device_generation: 1,
            snapshot: ManagerSnapshot::default(),
            now_us,
            last_root_state,
            completions: Vec::new(),
            root_scan: RootScan::Idle,
            root_speed: UsbSpeed::Full,
            ep0_packet_size: 8,
            control: ControlTransfer::new(),
            hub_scan: HubScan::Idle,
            hub_ports: Vec::new(),
            child_scan: ChildScan::Idle,
            child_ep0: 8,
            tt_recovery: None,
            tt_recovery_attempts: 0,
            hid_blocked: 0,
            hid: None,
            last_reset_status: None,
            last_reset_error: None,
            fallback_probe: None,
        }
    }
    pub fn allocate_device(
        &mut self,
        speed: UsbSpeed,
        location: DeviceLocation,
    ) -> Result<UsbAddress, UsbError> {
        if self.devices.len() >= MAX_DEVICES {
            return Err(UsbError::ResourceExhausted);
        }
        let address = self.addresses.allocate()?;
        let generation = self.next_device_generation;
        self.next_device_generation = self.next_device_generation.wrapping_add(1).max(1);
        self.devices.push(DeviceRecord {
            address,
            generation,
            speed,
            location,
            configuration: None,
            vendor_id: None,
            product_id: None,
            descriptor: Vec::new(),
        });
        self.snapshot.device_count = self.devices.len() as u8;
        self.snapshot.topology_generation = self.snapshot.topology_generation.wrapping_add(1);
        Ok(address)
    }
    pub fn detach(&mut self, address: UsbAddress) {
        if let Some(i) = self.devices.iter().position(|d| d.address == address) {
            self.devices.remove(i);
            self.addresses.release(address);
            self.snapshot.device_count = self.devices.len() as u8;
            self.snapshot.topology_generation = self.snapshot.topology_generation.wrapping_add(1);
            self.snapshot.disconnects = self.snapshot.disconnects.saturating_add(1)
        }
    }
    pub fn devices(&self) -> &[DeviceRecord] {
        &self.devices
    }
    pub const fn snapshot(&self) -> ManagerSnapshot {
        self.snapshot
    }
    pub fn keyboard_ready(&self) -> bool {
        self.hid.is_some()
    }
    fn child_enumerating(&self) -> bool {
        !matches!(self.child_scan, ChildScan::Idle | ChildScan::Running)
    }
    pub fn controller_snapshot(&self) -> super::hcd::HcdSnapshot {
        self.hcd.snapshot()
    }
    pub fn take_completion(&mut self) -> Option<TransferCompletion> {
        self.completions.pop()
    }
    fn poll_once(&mut self) {
        let now = (self.now_us)();
        // Deassert a due root reset (and publish transfer timeouts) before
        // sampling state, so the same poll can observe Enabled/completions.
        self.hcd.service_timeouts(now);
        let mut control_done = None;
        for _ in 0..COMPLETION_BUDGET {
            let Some(c) = self.hcd.reap() else { break };
            self.snapshot.completions = self.snapshot.completions.saturating_add(1);
            // A NAK or NYET means the device had nothing to say this service
            // interval, which is the normal answer from an idle keyboard a
            // hundred times a second. Counting it as an error buries the ones
            // that matter.
            if !matches!(c.result, Ok(_) | Err(UsbError::Nak | UsbError::Nyet)) {
                self.snapshot.errors = self.snapshot.errors.saturating_add(1)
            }
            if self
                .hid
                .as_ref()
                .is_some_and(|session| session.token == Some(c.token))
            {
                self.complete_hid(c, now);
            } else if self.control.owns(c.token) {
                if let Some(done) = self.control.complete(&mut *self.hcd, c, now) {
                    control_done = Some(done);
                    break;
                }
            } else {
                self.completions.push(c)
            }
        }
        if let Some(done) = control_done {
            if self.child_scan != ChildScan::Idle && self.child_scan != ChildScan::Running {
                self.advance_child(done, now);
            } else if self.hub_scan == HubScan::Idle {
                self.advance_enumeration(done, now);
            } else {
                self.advance_hub(done, now);
            }
        }
        let root = self.hcd.root_port_state();
        if root != self.last_root_state {
            crate::usb_println!("USB root port: {:?}", root);
        }
        if matches!(root, RootPortState::Disconnected)
            && !matches!(self.last_root_state, RootPortState::Disconnected)
        {
            self.cancel_hid();
            self.control.cancel(&mut *self.hcd);
            let addresses: Vec<_> = self.devices.iter().map(|d| d.address).collect();
            for address in addresses {
                self.detach(address)
            }
            self.root_scan = RootScan::Idle;
            self.hub_scan = HubScan::Idle;
            self.child_scan = ChildScan::Idle;
            self.tt_recovery = None;
            self.tt_recovery_attempts = 0;
            self.hub_ports.clear();
        }
        match (root, self.root_scan) {
            (RootPortState::Connected(_), RootScan::Idle) => {
                self.root_scan = RootScan::Debouncing {
                    until_us: now.saturating_add(100_000),
                };
            }
            (RootPortState::Connected(_), RootScan::Debouncing { until_us }) if now >= until_us => {
                let until_us = now.saturating_add(50_000);
                if self.hcd.reset_root_port(until_us).is_ok() {
                    crate::usb_println!("USB: resetting root port");
                    self.root_scan = RootScan::Resetting { until_us };
                } else {
                    self.snapshot.errors = self.snapshot.errors.saturating_add(1);
                    self.root_scan = RootScan::Idle;
                }
            }
            (RootPortState::Enabled(_), RootScan::Resetting { until_us }) if now >= until_us => {
                self.root_scan = RootScan::Ready;
            }
            (RootPortState::Enabled(speed), RootScan::Ready) => {
                self.root_speed = speed;
                let setup = SetupPacket::get_descriptor(DESCRIPTOR_DEVICE, 0, 0, 8);
                if self
                    .control
                    .start(&mut *self.hcd, UsbAddress::DEFAULT, speed, 8, setup, now)
                    .is_ok()
                {
                    crate::usb_println!("USB: root port enabled {:?}; enumerating", speed);
                    self.root_scan = RootScan::ReadingFirstDescriptor;
                }
            }
            (RootPortState::Connected(_), RootScan::Resetting { until_us })
                if now >= until_us.saturating_add(100_000) =>
            {
                crate::usb_println!(
                    "USB: root reset did not enable the port, HCD {:?}",
                    self.hcd.snapshot()
                );
                self.snapshot.errors = self.snapshot.errors.saturating_add(1);
                self.root_scan = RootScan::Debouncing {
                    until_us: now.saturating_add(1_000_000),
                };
            }
            (RootPortState::Enabled(_), RootScan::AddressDelay { address, until_us })
                if now >= until_us =>
            {
                if self
                    .start_descriptor(address, DESCRIPTOR_DEVICE, 18, now)
                    .is_ok()
                {
                    self.root_scan = RootScan::ReadingDevice { address };
                }
            }
            _ => {}
        }
        if let HubScan::WaitingForPower {
            hub,
            ports,
            until_us,
        } = self.hub_scan
            && now >= until_us
            && !self.control.is_active()
            && self.start_hub_port_status(hub, 1, now).is_ok()
        {
            self.hub_scan = HubScan::ReadingPortStatus {
                hub,
                port: 1,
                ports,
            };
        }
        if let HubScan::WaitingForReset {
            hub,
            port,
            ports,
            until_us,
        } = self.hub_scan
            && now >= until_us
            && !self.control.is_active()
            && !self.child_enumerating()
            && self.start_hub_port_status(hub, port, now).is_ok()
        {
            self.hub_scan = HubScan::ReadingResetStatus { hub, port, ports };
        }
        if let HubScan::ScanPaused { hub, port, ports } = self.hub_scan
            && !self.control.is_active()
            && !self.hid_poll_due(now)
        {
            self.continue_hub_port_scan(hub, port, ports, now);
        }
        if let HubScan::Monitoring {
            hub,
            ports,
            next_scan_us,
        } = self.hub_scan
            && now >= next_scan_us
            && !self.control.is_active()
            && !self.child_enumerating()
            && self
                .hid
                .as_ref()
                .is_none_or(|session| session.token.is_none())
        {
            for managed in &mut self.hub_ports {
                managed.poll(now);
            }
            if let Some(port) = self.hub_ports.iter().find_map(|managed| {
                matches!(
                    managed.phase,
                    super::class::hub::PortPhase::Resetting { .. }
                )
                .then_some(managed.number)
            }) {
                if self.start_hub_port_reset(hub, port, now).is_ok() {
                    self.hub_scan = HubScan::ResettingPort { hub, port, ports };
                }
            } else if self.start_hub_port_status(hub, 1, now).is_ok() {
                self.hub_scan = HubScan::ReadingPortStatus {
                    hub,
                    port: 1,
                    ports,
                };
            }
        }
        if let ChildScan::AddressDelay {
            hub,
            port,
            speed,
            address,
            until_us,
        } = self.child_scan
            && now >= until_us
            && !self.control.is_active()
        {
            let route = self.child_route(hub, port, speed);
            if self
                .start_child_descriptor(address, route, DESCRIPTOR_DEVICE, 18, now)
                .is_ok()
            {
                self.child_scan = ChildScan::ReadingDevice {
                    hub,
                    port,
                    speed,
                    address,
                };
            }
        }
        if let Some(recovery) = self.tt_recovery
            && let Some(retry_at_us) = recovery.retry_at_us
            && now >= retry_at_us
            && !self.control.is_active()
        {
            self.tt_recovery = None;
            let request = recovery.request;
            if self
                .control
                .start_routed(
                    &mut *self.hcd,
                    request.address,
                    request.route,
                    request.max_packet_size,
                    request.setup,
                    now,
                )
                .is_err()
            {
                self.child_failed();
            }
        }
        if let ChildScan::ResetRecovery {
            hub,
            port,
            speed,
            until_us,
        } = self.child_scan
            && now >= until_us
            && !self.control.is_active()
        {
            let route = self.child_route(hub, port, speed);
            let setup = SetupPacket::get_descriptor(DESCRIPTOR_DEVICE, 0, 0, 8);
            if self
                .control
                .start_routed(&mut *self.hcd, UsbAddress::DEFAULT, route, 8, setup, now)
                .is_ok()
            {
                self.child_scan = ChildScan::ReadingFirst { hub, port, speed };
            }
        }
        if let Some(session) = self.hid.as_mut()
            && now >= session.next_stats_us
        {
            session.next_stats_us = now.saturating_add(HID_STATS_INTERVAL_US);
            crate::usb_println!(
                "USB: keyboard t={} polls={} reports={} naks={} errors={} blocked={}",
                now / 1_000,
                session.polls,
                session.reports,
                session.naks,
                session.errors,
                self.hid_blocked,
            );
            crate::usb_println!(
                "USB: keys delivered={} rollover={} kbd_dropped={} console_dropped={}",
                session.keyboard.delivered(),
                session.keyboard.rollover_count(),
                session.keyboard.dropped(),
                super::input::dropped(),
            );
            let hcd = self.hcd.snapshot();
            crate::usb_println!(
                "USB: split csplit_retries={} expired={} exhausted={} naks={} deferrals={} | completed={} stalls={} timeouts={} xact={}",
                hcd.split_csplit_retries,
                hcd.split_expired,
                hcd.split_exhausted,
                hcd.split_naks,
                hcd.split_deferrals,
                hcd.completed,
                hcd.stalls,
                hcd.timeouts,
                hcd.transaction_errors,
            );
            crate::usb_println!(
                "USB: periodic by start uframe: started={:?} lost={:?}",
                hcd.periodic_starts,
                hcd.periodic_losses,
            );
        }
        if self
            .hid
            .as_ref()
            .is_some_and(|session| session.token.is_none() && now >= session.next_poll_us)
        {
            if self.control.is_active() {
                // A keyboard poll is due but a control transfer owns the bus,
                // so this service interval is skipped entirely. A long run of
                // these is how a hub scan or a recovery turns into dropped
                // keys.
                self.hid_blocked = self.hid_blocked.saturating_add(1);
            } else {
                let _ = self.submit_hid(now);
            }
        }
        self.last_root_state = root;
    }

    fn start_descriptor(
        &mut self,
        address: UsbAddress,
        descriptor_type: u8,
        length: u16,
        now: u64,
    ) -> Result<(), UsbError> {
        self.control.start(
            &mut *self.hcd,
            address,
            self.root_speed,
            self.ep0_packet_size,
            SetupPacket::get_descriptor(descriptor_type, 0, 0, length),
            now,
        )
    }

    fn enumeration_failed(&mut self, now: u64) {
        crate::usb_println!(
            "USB: root enumeration failed at {:?}, HCD {:?}",
            self.root_scan,
            self.hcd.snapshot()
        );
        self.snapshot.errors = self.snapshot.errors.saturating_add(1);
        if let Some(address) = self.devices.iter().find_map(|device| {
            matches!(device.location, DeviceLocation::Root).then_some(device.address)
        }) {
            self.detach(address);
        }
        self.root_scan = RootScan::Debouncing {
            until_us: now.saturating_add(1_000_000),
        };
    }

    fn advance_enumeration(&mut self, result: Result<usize, UsbError>, now: u64) {
        let Ok(actual) = result else {
            self.enumeration_failed(now);
            return;
        };
        let next = match self.root_scan {
            RootScan::ReadingFirstDescriptor => {
                if actual < 8 || !matches!(self.control.data[7], 8 | 16 | 32 | 64) {
                    self.enumeration_failed(now);
                    return;
                }
                self.ep0_packet_size = self.control.data[7] as u16;
                let Ok(address) = self.allocate_device(self.root_speed, DeviceLocation::Root)
                else {
                    self.enumeration_failed(now);
                    return;
                };
                let setup = SetupPacket::set_address(address.get());
                if self
                    .control
                    .start(
                        &mut *self.hcd,
                        UsbAddress::DEFAULT,
                        self.root_speed,
                        self.ep0_packet_size,
                        setup,
                        now,
                    )
                    .is_err()
                {
                    self.enumeration_failed(now);
                    return;
                }
                RootScan::SettingAddress { address }
            }
            RootScan::SettingAddress { address } => RootScan::AddressDelay {
                address,
                until_us: now.saturating_add(2_000),
            },
            RootScan::ReadingDevice { address } => {
                let Ok(descriptor) = DeviceDescriptor::parse(&self.control.data[..actual]) else {
                    self.enumeration_failed(now);
                    return;
                };
                if let Some(record) = self.devices.iter_mut().find(|d| d.address == address) {
                    record.vendor_id = Some(descriptor.vendor_id);
                    record.product_id = Some(descriptor.product_id);
                    record.descriptor.clear();
                    record
                        .descriptor
                        .extend_from_slice(&self.control.data[..actual]);
                }
                if self
                    .start_descriptor(address, DESCRIPTOR_CONFIGURATION, 9, now)
                    .is_err()
                {
                    self.enumeration_failed(now);
                    return;
                }
                RootScan::ReadingConfigHeader { address }
            }
            RootScan::ReadingConfigHeader { address } => {
                let Ok(config) = ConfigurationDescriptor::parse(&self.control.data[..actual])
                else {
                    self.enumeration_failed(now);
                    return;
                };
                let length = config.total_length.min(MAX_CONFIGURATION_DESCRIPTOR as u16);
                if self
                    .start_descriptor(address, DESCRIPTOR_CONFIGURATION, length, now)
                    .is_err()
                {
                    self.enumeration_failed(now);
                    return;
                }
                RootScan::ReadingConfig { address }
            }
            RootScan::ReadingConfig { address } => {
                let Ok(config) = ConfigurationDescriptor::parse(&self.control.data[..actual])
                else {
                    self.enumeration_failed(now);
                    return;
                };
                if let Some(record) = self.devices.iter_mut().find(|d| d.address == address) {
                    record.descriptor.clear();
                    record
                        .descriptor
                        .extend_from_slice(&self.control.data[..actual]);
                }
                if self
                    .control
                    .start(
                        &mut *self.hcd,
                        address,
                        self.root_speed,
                        self.ep0_packet_size,
                        SetupPacket::set_configuration(config.value),
                        now,
                    )
                    .is_err()
                {
                    self.enumeration_failed(now);
                    return;
                }
                RootScan::SettingConfiguration {
                    address,
                    value: config.value,
                }
            }
            RootScan::SettingConfiguration { address, value } => {
                if let Some(record) = self.devices.iter_mut().find(|d| d.address == address) {
                    record.configuration = Some(value);
                }
                let is_hub = self
                    .devices
                    .iter()
                    .find(|d| d.address == address)
                    .is_some_and(|record| {
                        DescriptorIter::new(&record.descriptor).any(|item| {
                            item.ok().is_some_and(|descriptor| {
                                descriptor.descriptor_type == 4
                                    && descriptor.bytes.len() >= 9
                                    && descriptor.bytes[5] == 9
                            })
                        })
                    });
                if is_hub {
                    crate::usb_println!("USB: hub configured at address {}", address.get());
                    // Hub descriptors have variable-length removable/power
                    // bitmaps. Ask for the bounded maximum rather than
                    // truncating hubs with more than seven ports.
                    let request = SetupPacket::new(0xa0, 6, (DESCRIPTOR_HUB as u16) << 8, 0, 64);
                    if self
                        .control
                        .start(
                            &mut *self.hcd,
                            address,
                            self.root_speed,
                            self.ep0_packet_size,
                            request,
                            now,
                        )
                        .is_ok()
                    {
                        self.hub_scan = HubScan::ReadingDescriptor { hub: address };
                    }
                }
                RootScan::Configured
            }
            _ => self.root_scan,
        };
        self.root_scan = next;
    }

    fn start_hub_port_power(
        &mut self,
        hub: UsbAddress,
        port: u8,
        now: u64,
    ) -> Result<(), UsbError> {
        self.control.start(
            &mut *self.hcd,
            hub,
            self.devices
                .iter()
                .find(|device| device.address == hub)
                .map_or(UsbSpeed::High, |device| device.speed),
            self.ep0_packet_size,
            SetupPacket::new(0x23, 3, 8, port as u16, 0),
            now,
        )
    }

    fn start_hub_port_status(
        &mut self,
        hub: UsbAddress,
        port: u8,
        now: u64,
    ) -> Result<(), UsbError> {
        self.control.start(
            &mut *self.hcd,
            hub,
            self.devices
                .iter()
                .find(|device| device.address == hub)
                .map_or(UsbSpeed::High, |device| device.speed),
            self.ep0_packet_size,
            SetupPacket::new(0xa3, 0, 0, port as u16, 4),
            now,
        )
    }

    fn start_hub_port_reset(
        &mut self,
        hub: UsbAddress,
        port: u8,
        now: u64,
    ) -> Result<(), UsbError> {
        self.control.start(
            &mut *self.hcd,
            hub,
            self.devices
                .iter()
                .find(|device| device.address == hub)
                .map_or(UsbSpeed::High, |device| device.speed),
            self.ep0_packet_size,
            SetupPacket::new(0x23, 3, 4, port as u16, 0),
            now,
        )
    }

    fn start_hub_clear_port_feature(
        &mut self,
        hub: UsbAddress,
        port: u8,
        feature: u16,
        now: u64,
    ) -> Result<(), UsbError> {
        self.control.start(
            &mut *self.hcd,
            hub,
            self.devices
                .iter()
                .find(|device| device.address == hub)
                .map_or(UsbSpeed::High, |device| device.speed),
            self.ep0_packet_size,
            SetupPacket::new(0x23, 1, feature, port as u16, 0),
            now,
        )
    }

    fn continue_hub_port_scan(&mut self, hub: UsbAddress, port: u8, ports: u8, now: u64) {
        if port < ports && self.hid_poll_due(now) {
            // The scan walks every port in one unbroken run of control
            // transfers, and a keyboard poll cannot start while one is in
            // flight. On a five-port hub that is long enough to swallow a
            // whole key press, so give the keyboard its interval first and
            // pick the scan up afterwards.
            self.hub_scan = HubScan::ScanPaused { hub, port, ports };
            return;
        }
        if port < ports {
            let next = port + 1;
            if self.start_hub_port_status(hub, next, now).is_ok() {
                self.hub_scan = HubScan::ReadingPortStatus {
                    hub,
                    port: next,
                    ports,
                };
                return;
            }
        }
        self.hub_scan = HubScan::Monitoring {
            hub,
            ports,
            next_scan_us: now.saturating_add(HUB_MONITOR_INTERVAL_US),
        };
    }

    /// True when the keyboard is due for a poll and is not already in flight.
    fn hid_poll_due(&self, now: u64) -> bool {
        self.hid
            .as_ref()
            .is_some_and(|session| session.token.is_none() && now >= session.next_poll_us)
    }

    fn arm_child_after_reset(&mut self, hub: UsbAddress, port: u8, speed: UsbSpeed, now: u64) {
        if self.child_scan == ChildScan::Idle {
            self.tt_recovery = None;
            self.tt_recovery_attempts = 0;
            self.child_scan = ChildScan::ResetRecovery {
                hub,
                port,
                speed,
                until_us: now.saturating_add(10_000),
            };
        }
    }

    fn advance_hub(&mut self, result: Result<usize, UsbError>, now: u64) {
        let actual = match result {
            Ok(actual) => actual,
            Err(error) => {
                if let HubScan::ReadingPortStatus { hub, port, ports } = self.hub_scan {
                    // A single port can transiently reject GET_STATUS (the
                    // LAN951x internal Ethernet port does this on real Pi 3
                    // hardware). Do not abort the whole scan before reaching
                    // external ports such as port 5, and do not flood UART
                    // once per monitoring interval.
                    self.snapshot.errors = self.snapshot.errors.saturating_add(1);
                    self.continue_hub_port_scan(hub, port, ports, now);
                    return;
                }
                if let HubScan::ClearingConnectionChange { hub, port, ports } = self.hub_scan {
                    self.snapshot.errors = self.snapshot.errors.saturating_add(1);
                    self.continue_hub_port_scan(hub, port, ports, now);
                    return;
                }
                if let HubScan::ClearingResetChange {
                    hub,
                    port,
                    ports,
                    speed,
                } = self.hub_scan
                {
                    self.snapshot.errors = self.snapshot.errors.saturating_add(1);
                    self.arm_child_after_reset(hub, port, speed, now);
                    self.hub_scan = HubScan::Monitoring {
                        hub,
                        ports,
                        next_scan_us: now.saturating_add(HUB_MONITOR_INTERVAL_US),
                    };
                    return;
                }
                if let HubScan::ReadingResetStatus { hub, port, ports } = self.hub_scan {
                    let interrupt = self.hcd.snapshot().last_interrupt;
                    let observation = (hub, port, interrupt);
                    if self.last_reset_error != Some(observation) {
                        crate::usb_println!(
                            "USB: hub {} port {} reset status failed: {:?}, HCINT={:08x}",
                            hub.get(),
                            port,
                            error,
                            interrupt
                        );
                        self.last_reset_error = Some(observation);
                    }
                    self.snapshot.errors = self.snapshot.errors.saturating_add(1);
                    if let Some((probe_hub, probe_port, speed)) = self.fallback_probe
                        && probe_hub == hub
                        && probe_port == port
                    {
                        // SET_FEATURE(PORT_RESET) completed and the 50 ms
                        // recovery delay elapsed. Some LAN9514 revisions then
                        // reject GET_STATUS with STALL+DTERR. The previously
                        // known child speed is sufficient to try address zero.
                        self.arm_child_after_reset(hub, port, speed, now);
                        self.hub_scan = HubScan::Monitoring {
                            hub,
                            ports,
                            next_scan_us: now.saturating_add(HUB_MONITOR_INTERVAL_US),
                        };
                        return;
                    }
                    self.hub_scan = HubScan::Monitoring {
                        hub,
                        ports,
                        next_scan_us: now.saturating_add(HUB_MONITOR_INTERVAL_US),
                    };
                    return;
                }
                if !matches!(
                    self.hub_scan,
                    HubScan::ResettingPort { .. } | HubScan::ReadingResetStatus { .. }
                ) {
                    crate::usb_println!(
                        "USB: hub operation failed at {:?}, HCD {:?}",
                        self.hub_scan,
                        self.hcd.snapshot()
                    );
                }
                self.snapshot.errors = self.snapshot.errors.saturating_add(1);
                // Once a hub has been configured, a transient GET_STATUS/reset
                // failure must not disable hot-plug monitoring forever. Restart
                // the scan later from port 1. Descriptor/power failures still
                // fall back to Idle because the hub is not operational yet.
                self.hub_scan = match self.hub_scan {
                    HubScan::ReadingPortStatus { hub, ports, .. }
                    | HubScan::ResettingPort { hub, ports, .. }
                    | HubScan::ReadingResetStatus { hub, ports, .. } => HubScan::Monitoring {
                        hub,
                        ports,
                        next_scan_us: now.saturating_add(HUB_MONITOR_INTERVAL_US),
                    },
                    _ => HubScan::Idle,
                };
                return;
            }
        };
        match self.hub_scan {
            HubScan::ReadingDescriptor { hub } => {
                let Ok(descriptor) = HubDescriptor::parse(&self.control.data[..actual]) else {
                    crate::usb_println!(
                        "USB: invalid hub descriptor ({} bytes: {:02x?})",
                        actual,
                        &self.control.data[..actual.min(9)]
                    );
                    self.snapshot.errors = self.snapshot.errors.saturating_add(1);
                    self.hub_scan = HubScan::Idle;
                    return;
                };
                let ports = descriptor.ports.min(MAX_DOWNSTREAM_PORTS);
                crate::usb_println!("USB: hub {} has {} managed ports", hub.get(), ports);
                self.hub_ports.clear();
                for port in 1..=ports {
                    self.hub_ports.push(HubPort::new(port));
                }
                let power_good_us = descriptor.power_on_to_good_2ms as u64 * 2_000;
                if ports != 0 && self.start_hub_port_power(hub, 1, now).is_ok() {
                    self.hub_scan = HubScan::PoweringPorts {
                        hub,
                        port: 1,
                        ports,
                        power_good_us,
                    };
                }
            }
            HubScan::PoweringPorts {
                hub,
                port,
                ports,
                power_good_us,
            } => {
                if port < ports {
                    let next = port + 1;
                    if self.start_hub_port_power(hub, next, now).is_ok() {
                        self.hub_scan = HubScan::PoweringPorts {
                            hub,
                            port: next,
                            ports,
                            power_good_us,
                        };
                    }
                } else {
                    self.hub_scan = HubScan::WaitingForPower {
                        hub,
                        ports,
                        until_us: now.saturating_add(power_good_us),
                    };
                }
            }
            HubScan::ReadingPortStatus { hub, port, ports } => {
                let mut change = 0;
                if actual >= 4 {
                    let status = u16::from_le_bytes([self.control.data[0], self.control.data[1]]);
                    change = u16::from_le_bytes([self.control.data[2], self.control.data[3]]);
                    let connected = status & 1 != 0;
                    let was_connected =
                        self.hub_ports
                            .get(port as usize - 1)
                            .is_some_and(|managed| {
                                !matches!(managed.phase, super::class::hub::PortPhase::Disconnected)
                            });
                    if let Some(managed) = self.hub_ports.get_mut(port as usize - 1) {
                        if connected != was_connected {
                            crate::usb_println!(
                                "USB: hub {} port {} {} (status {:04x})",
                                hub.get(),
                                port,
                                if connected {
                                    "connected"
                                } else {
                                    "disconnected"
                                },
                                status
                            );
                            managed.connection_changed(connected, now);
                        }
                    }
                    if was_connected && !connected {
                        self.detach_hub_port(hub, port);
                    }
                }
                if change & 1 != 0
                    && self
                        .start_hub_clear_port_feature(hub, port, 16, now)
                        .is_ok()
                {
                    self.hub_scan = HubScan::ClearingConnectionChange { hub, port, ports };
                    return;
                }
                self.continue_hub_port_scan(hub, port, ports, now);
            }
            HubScan::ClearingConnectionChange { hub, port, ports } => {
                self.continue_hub_port_scan(hub, port, ports, now);
            }
            HubScan::ResettingPort { hub, port, ports } => {
                // Start the recovery interval when SET_FEATURE(PORT_RESET)
                // has actually completed.  A port can remain queued behind
                // enumeration of an earlier child for a long time, so the
                // deadline assigned when debounce ended may already be in
                // the past here.  Reusing that stale deadline causes an
                // immediate GET_STATUS followed by another reset forever on
                // real hubs (QEMU completes reset synchronously and hides it).
                let until_us = now.saturating_add(50_000);
                if let Some(managed) = self.hub_ports.get_mut(port as usize - 1) {
                    managed.phase = super::class::hub::PortPhase::Resetting { until_us };
                }
                crate::usb_println!("USB: hub {} port {} resetting", hub.get(), port);
                self.hub_scan = HubScan::WaitingForReset {
                    hub,
                    port,
                    ports,
                    until_us,
                };
            }
            HubScan::ReadingResetStatus { hub, port, ports } => {
                if actual >= 4 {
                    let status = u16::from_le_bytes([self.control.data[0], self.control.data[1]]);
                    let change = u16::from_le_bytes([self.control.data[2], self.control.data[3]]);
                    let observation = (hub, port, status, change);
                    if status & 3 != 3 && self.last_reset_status != Some(observation) {
                        crate::usb_println!(
                            "USB: hub {} port {} reset incomplete: status {:04x}, change {:04x}",
                            hub.get(),
                            port,
                            status,
                            change
                        );
                        self.last_reset_status = Some(observation);
                    }
                    if status & 0x10 != 0 {
                        // Reset signaling is still active. Poll its status
                        // again instead of issuing another PORT_RESET.
                        self.hub_scan = HubScan::WaitingForReset {
                            hub,
                            port,
                            ports,
                            until_us: now.saturating_add(10_000),
                        };
                        return;
                    }
                    if status & 3 == 3 {
                        self.last_reset_status = None;
                        self.last_reset_error = None;
                        let speed = match (status >> 9) & 3 {
                            1 => UsbSpeed::Low,
                            2 => UsbSpeed::High,
                            _ => UsbSpeed::Full,
                        };
                        if let Some(managed) = self.hub_ports.get_mut(port as usize - 1) {
                            managed.phase = super::class::hub::PortPhase::Active(speed);
                        }
                        crate::usb_println!(
                            "USB: hub {} port {} enabled {:?} (status {:04x})",
                            hub.get(),
                            port,
                            speed,
                            status
                        );
                        if self
                            .start_hub_clear_port_feature(hub, port, 20, now)
                            .is_ok()
                        {
                            self.hub_scan = HubScan::ClearingResetChange {
                                hub,
                                port,
                                ports,
                                speed,
                            };
                            return;
                        }
                        self.arm_child_after_reset(hub, port, speed, now);
                    } else if let Some(managed) = self.hub_ports.get_mut(port as usize - 1) {
                        // A reset probe of an empty port can complete at the
                        // hub even though no child is present. Stop actively
                        // resetting it; the normal status scan will arm a new
                        // debounce when a device is inserted.
                        managed.phase = if status & 1 == 0 {
                            super::class::hub::PortPhase::Disconnected
                        } else {
                            super::class::hub::PortPhase::Debouncing {
                                until_us: now.saturating_add(100_000),
                            }
                        };
                    }
                }
                self.hub_scan = HubScan::Monitoring {
                    hub,
                    ports,
                    next_scan_us: now.saturating_add(HUB_MONITOR_INTERVAL_US),
                };
            }
            HubScan::ClearingResetChange {
                hub,
                port,
                ports,
                speed,
            } => {
                self.arm_child_after_reset(hub, port, speed, now);
                self.hub_scan = HubScan::Monitoring {
                    hub,
                    ports,
                    next_scan_us: now.saturating_add(HUB_MONITOR_INTERVAL_US),
                };
            }
            _ => {}
        }
    }

    fn child_route(&self, hub: UsbAddress, port: u8, speed: UsbSpeed) -> UsbRoute {
        let hub_speed = self
            .devices
            .iter()
            .find(|device| device.address == hub)
            .map_or(UsbSpeed::High, |device| device.speed);
        UsbRoute {
            device_speed: speed,
            translator: (hub_speed == UsbSpeed::High && speed != UsbSpeed::High).then_some(
                SplitTarget {
                    hub_address: hub,
                    port_number: port,
                    hub_speed,
                },
            ),
        }
    }

    fn start_child_descriptor(
        &mut self,
        address: UsbAddress,
        route: UsbRoute,
        kind: u8,
        length: u16,
        now: u64,
    ) -> Result<(), UsbError> {
        self.control.start_routed(
            &mut *self.hcd,
            address,
            route,
            self.child_ep0,
            SetupPacket::get_descriptor(kind, 0, 0, length),
            now,
        )
    }

    fn find_boot_keyboard(bytes: &[u8]) -> Option<(u8, EndpointDescriptor)> {
        let mut interface = None;
        for item in DescriptorIter::new(bytes) {
            let descriptor = item.ok()?;
            match descriptor.descriptor_type {
                4 => {
                    let parsed = InterfaceDescriptor::parse(descriptor.bytes).ok()?;
                    interface = (parsed.class == 3 && parsed.subclass == 1 && parsed.protocol == 1)
                        .then_some(parsed.number);
                }
                5 if interface.is_some() => {
                    let endpoint = EndpointDescriptor::parse(descriptor.bytes).ok()?;
                    if endpoint.transfer_type == TransferType::Interrupt
                        && endpoint.address.direction() == Direction::In
                    {
                        return Some((interface.unwrap(), endpoint));
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn child_failed(&mut self) {
        crate::usb_println!("USB: child enumeration failed at {:?}", self.child_scan);
        self.snapshot.errors = self.snapshot.errors.saturating_add(1);
        let failed_port = match self.child_scan {
            ChildScan::ResetRecovery { hub, port, .. }
            | ChildScan::ReadingFirst { hub, port, .. }
            | ChildScan::SettingAddress { hub, port, .. }
            | ChildScan::AddressDelay { hub, port, .. }
            | ChildScan::ReadingDevice { hub, port, .. }
            | ChildScan::ReadingConfigHeader { hub, port, .. }
            | ChildScan::ReadingConfig { hub, port, .. }
            | ChildScan::SettingConfiguration { hub, port, .. } => Some((hub, port)),
            ChildScan::SettingProtocol { address, route, .. }
            | ChildScan::SettingIdle { address, route, .. } => route
                .translator
                .map(|tt| (tt.hub_address, tt.port_number))
                .or_else(|| {
                    self.devices.iter().find_map(|device| {
                        (device.address == address)
                            .then_some(device.location)
                            .and_then(|location| match location {
                                DeviceLocation::Hub { hub, port } => Some((hub, port)),
                                DeviceLocation::Root => None,
                            })
                    })
                }),
            _ => None,
        };
        let address = match self.child_scan {
            ChildScan::SettingAddress { address, .. }
            | ChildScan::AddressDelay { address, .. }
            | ChildScan::ReadingDevice { address, .. }
            | ChildScan::ReadingConfigHeader { address, .. }
            | ChildScan::ReadingConfig { address, .. }
            | ChildScan::SettingConfiguration { address, .. }
            | ChildScan::SettingProtocol { address, .. }
            | ChildScan::SettingIdle { address, .. } => Some(address),
            _ => None,
        };
        if let Some(address) = address {
            self.detach(address);
        }
        self.tt_recovery = None;
        self.tt_recovery_attempts = 0;
        self.child_scan = ChildScan::Idle;
        if let Some((hub, port)) = failed_port
            && self.devices.iter().any(|device| device.address == hub)
            && let Some(managed) = self.hub_ports.get_mut(port as usize - 1)
        {
            // A device can need another reset after a quick unplug/replug,
            // especially when the previous interrupt channel was cancelled
            // mid-transaction. Re-enter debounce so monitoring retries the
            // port instead of leaving it permanently Active.
            managed.connection_changed(true, (self.now_us)());
        }
    }

    fn cancel_hid(&mut self) {
        if let Some(mut session) = self.hid.take()
            && let Some(token) = session.token.take()
        {
            let _ = self.hcd.cancel(token);
        }
    }

    fn detach_hub_port(&mut self, hub: UsbAddress, port: u8) {
        let addresses: Vec<_> = self
            .devices
            .iter()
            .filter_map(|device| {
                (device.location == DeviceLocation::Hub { hub, port }).then_some(device.address)
            })
            .collect();
        if self
            .hid
            .as_ref()
            .is_some_and(|session| addresses.contains(&session.address))
        {
            self.cancel_hid();
        }
        for address in addresses {
            self.detach(address);
        }
        self.child_scan = ChildScan::Idle;
    }

    fn clear_tt_setup(devinfo: u16, port: u8) -> SetupPacket {
        // USB 2.0 section 11.24.2.3: class/other CLEAR_TT_BUFFER.
        SetupPacket::new(0x23, 8, devinfo, port as u16, 0)
    }

    fn begin_tt_recovery(&mut self, now: u64) -> bool {
        if self.tt_recovery_attempts >= 3 {
            return false;
        }
        let request = self.control.request_context();
        let Some(target) = request.route.translator else {
            return false;
        };
        // CLEAR_TT_BUFFER encodes endpoint, device address, endpoint type,
        // and direction in wValue.  Endpoint zero is control type (zero).
        let direction = u16::from(request.setup.direction() == Direction::In) << 15;
        let devinfo = ((request.address.get() as u16) << 4) | direction;
        // A control endpoint owns buffers in both directions. Linux clears
        // the opposite direction first, then the direction of the request.
        let setup = Self::clear_tt_setup(devinfo ^ 0x8000, target.port_number);
        if self
            .control
            .start(
                &mut *self.hcd,
                target.hub_address,
                target.hub_speed,
                64,
                setup,
                now,
            )
            .is_err()
        {
            return false;
        }
        self.tt_recovery_attempts += 1;
        self.tt_recovery = Some(TtRecovery {
            request,
            hub: target.hub_address,
            port: target.port_number,
            devinfo,
            clearing_second_direction: false,
            retry_at_us: None,
        });
        crate::usb_println!(
            "USB: clearing TT hub {} port {} (attempt {})",
            target.hub_address.get(),
            target.port_number,
            self.tt_recovery_attempts
        );
        true
    }

    fn advance_tt_recovery(&mut self, result: Result<usize, UsbError>, now: u64) {
        let Some(mut recovery) = self.tt_recovery else {
            return;
        };
        if let Err(error) = result {
            crate::usb_println!("USB: CLEAR_TT_BUFFER failed: {:?}", error);
            self.tt_recovery = None;
            self.child_failed();
            return;
        }
        if !recovery.clearing_second_direction {
            let setup = Self::clear_tt_setup(recovery.devinfo, recovery.port);
            if self
                .control
                .start(&mut *self.hcd, recovery.hub, UsbSpeed::High, 64, setup, now)
                .is_err()
            {
                self.tt_recovery = None;
                self.child_failed();
                return;
            }
            recovery.clearing_second_direction = true;
            self.tt_recovery = Some(recovery);
            return;
        }
        // Although the CLEAR_TT_BUFFER control transfer has completed, the
        // LAN9514 can still report the old TT result for the next microframes.
        // Give it a short recovery interval before restarting the original
        // control request from SETUP.
        recovery.retry_at_us = Some(now.saturating_add(TT_RECOVERY_DELAY_US));
        self.tt_recovery = Some(recovery);
    }

    fn advance_child(&mut self, result: Result<usize, UsbError>, now: u64) {
        if self.tt_recovery.is_some() {
            self.advance_tt_recovery(result, now);
            return;
        }
        let actual = match result {
            Ok(actual) => actual,
            Err(error) => {
                let hcd = self.hcd.snapshot();
                let (stage, setup, actual, retries) = self.control.failure_context();
                crate::usb_println!(
                    "USB: child transfer failed: {:?}, HCINT={:08x}, sub={}, reap={}, active={}",
                    error,
                    hcd.last_interrupt,
                    hcd.submitted,
                    hcd.reaped,
                    hcd.active
                );
                crate::usb_println!(
                    "USB: control {} rt={:02x} req={:02x} val={:04x} len={} actual={} retries={}",
                    stage,
                    setup.request_type,
                    setup.request,
                    setup.value,
                    setup.length,
                    actual,
                    retries
                );
                if matches!(
                    error,
                    UsbError::Stall | UsbError::Transaction | UsbError::ControllerFault
                ) && self.begin_tt_recovery(now)
                {
                    return;
                }
                self.child_failed();
                return;
            }
        };
        let next = match self.child_scan {
            ChildScan::ReadingFirst { hub, port, speed } => {
                if actual < 8 || !matches!(self.control.data[7], 8 | 16 | 32 | 64) {
                    self.child_failed();
                    return;
                }
                self.child_ep0 = self.control.data[7] as u16;
                let Ok(address) = self.allocate_device(speed, DeviceLocation::Hub { hub, port })
                else {
                    self.child_failed();
                    return;
                };
                let route = self.child_route(hub, port, speed);
                if self
                    .control
                    .start_routed(
                        &mut *self.hcd,
                        UsbAddress::DEFAULT,
                        route,
                        self.child_ep0,
                        SetupPacket::set_address(address.get()),
                        now,
                    )
                    .is_err()
                {
                    self.child_failed();
                    return;
                }
                ChildScan::SettingAddress {
                    hub,
                    port,
                    speed,
                    address,
                }
            }
            ChildScan::SettingAddress {
                hub,
                port,
                speed,
                address,
            } => ChildScan::AddressDelay {
                hub,
                port,
                speed,
                address,
                // Give the device ample time to switch endpoint zero from
                // the default address, including after a hot-plug reset.
                until_us: now.saturating_add(10_000),
            },
            ChildScan::ReadingDevice {
                hub,
                port,
                speed,
                address,
            } => {
                let Ok(device) = DeviceDescriptor::parse(&self.control.data[..actual]) else {
                    self.child_failed();
                    return;
                };
                if let Some(record) = self.devices.iter_mut().find(|d| d.address == address) {
                    record.vendor_id = Some(device.vendor_id);
                    record.product_id = Some(device.product_id);
                }
                crate::usb_println!(
                    "USB: device {} on hub {} port {}: {:04x}:{:04x}",
                    address.get(),
                    hub.get(),
                    port,
                    device.vendor_id,
                    device.product_id
                );
                let route = self.child_route(hub, port, speed);
                if self
                    .start_child_descriptor(address, route, DESCRIPTOR_CONFIGURATION, 9, now)
                    .is_err()
                {
                    self.child_failed();
                    return;
                }
                ChildScan::ReadingConfigHeader {
                    hub,
                    port,
                    speed,
                    address,
                }
            }
            ChildScan::ReadingConfigHeader {
                hub,
                port,
                speed,
                address,
            } => {
                let Ok(config) = ConfigurationDescriptor::parse(&self.control.data[..actual])
                else {
                    self.child_failed();
                    return;
                };
                let route = self.child_route(hub, port, speed);
                crate::usb_println!(
                    "USB: device {} configuration length {}",
                    address.get(),
                    config.total_length
                );
                if self
                    .start_child_descriptor(
                        address,
                        route,
                        DESCRIPTOR_CONFIGURATION,
                        config.total_length.min(512),
                        now,
                    )
                    .is_err()
                {
                    self.child_failed();
                    return;
                }
                ChildScan::ReadingConfig {
                    hub,
                    port,
                    speed,
                    address,
                }
            }
            ChildScan::ReadingConfig {
                hub,
                port,
                speed,
                address,
            } => {
                let Ok(config) = ConfigurationDescriptor::parse(&self.control.data[..actual])
                else {
                    self.child_failed();
                    return;
                };
                if let Some(record) = self.devices.iter_mut().find(|d| d.address == address) {
                    record.descriptor.clear();
                    record
                        .descriptor
                        .extend_from_slice(&self.control.data[..actual]);
                }
                let route = self.child_route(hub, port, speed);
                if self
                    .control
                    .start_routed(
                        &mut *self.hcd,
                        address,
                        route,
                        self.child_ep0,
                        SetupPacket::set_configuration(config.value),
                        now,
                    )
                    .is_err()
                {
                    self.child_failed();
                    return;
                }
                ChildScan::SettingConfiguration {
                    hub,
                    port,
                    speed,
                    address,
                    value: config.value,
                }
            }
            ChildScan::SettingConfiguration {
                hub,
                port,
                speed,
                address,
                value,
            } => {
                if let Some(record) = self.devices.iter_mut().find(|d| d.address == address) {
                    record.configuration = Some(value);
                }
                let Some(record) = self.devices.iter().find(|d| d.address == address) else {
                    self.child_failed();
                    return;
                };
                let Some((interface, endpoint)) = Self::find_boot_keyboard(&record.descriptor)
                else {
                    crate::usb_println!("USB: device {} configured (non-keyboard)", address.get());
                    // This port is Active, so returning to Idle permits a
                    // different connected hub port to be enumerated without
                    // revisiting this device.
                    self.child_scan = ChildScan::Idle;
                    return;
                };
                let route = self.child_route(hub, port, speed);
                if self
                    .control
                    .start_routed(
                        &mut *self.hcd,
                        address,
                        route,
                        self.child_ep0,
                        set_protocol_boot(interface),
                        now,
                    )
                    .is_err()
                {
                    self.child_failed();
                    return;
                }
                ChildScan::SettingProtocol {
                    address,
                    route,
                    interface,
                    endpoint,
                }
            }
            ChildScan::SettingProtocol {
                address,
                route,
                interface,
                endpoint,
            } => {
                if self
                    .control
                    .start_routed(
                        &mut *self.hcd,
                        address,
                        route,
                        self.child_ep0,
                        set_idle(interface, IDLE_DURATION_4MS),
                        now,
                    )
                    .is_err()
                {
                    self.child_failed();
                    return;
                }
                ChildScan::SettingIdle {
                    address,
                    route,
                    endpoint,
                }
            }
            ChildScan::SettingIdle {
                address,
                route,
                endpoint,
            } => {
                crate::usb_println!(
                    "USB: Boot keyboard ready at address {}, endpoint {}, interval {} ms",
                    address.get(),
                    endpoint.address.number(),
                    endpoint.interval.max(1)
                );
                self.hid = Some(HidSession {
                    address,
                    route,
                    endpoint,
                    report: [0; 64],
                    token: None,
                    next_poll_us: now,
                    keyboard: BootKeyboard::new(),
                    pid: DataPid::Data0,
                    consecutive_errors: 0,
                    polls: 0,
                    reports: 0,
                    naks: 0,
                    errors: 0,
                    next_stats_us: now.saturating_add(HID_STATS_INTERVAL_US),
                });
                self.fallback_probe = None;
                ChildScan::Running
            }
            _ => self.child_scan,
        };
        self.child_scan = next;
    }

    fn submit_hid(&mut self, now: u64) -> Result<(), UsbError> {
        let session = self.hid.as_mut().ok_or(UsbError::InvalidRequest)?;
        let length = (session.endpoint.max_packet_size as usize).min(session.report.len());
        let token = self.hcd.submit(TransferRequest {
            address: session.address,
            endpoint: session.endpoint.address,
            transfer_type: TransferType::Interrupt,
            direction: Direction::In,
            route: session.route,
            max_packet_size: session.endpoint.max_packet_size,
            pid: session.pid,
            buffer: &mut session.report[..length],
            deadline_us: now.saturating_add(1_000_000),
        })?;
        session.token = Some(token);
        session.polls = session.polls.saturating_add(1);
        Ok(())
    }

    fn complete_hid(&mut self, completion: TransferCompletion, now: u64) {
        let Some(session) = self.hid.as_mut() else {
            return;
        };
        session.token = None;
        let mut disconnected = None;
        match completion.result {
            Ok(actual) => {
                session.consecutive_errors = 0;
                session.reports = session.reports.saturating_add(1);
                session.pid = match session.pid {
                    DataPid::Data0 => DataPid::Data1,
                    DataPid::Data1 => DataPid::Data0,
                    DataPid::Setup => DataPid::Data0,
                };
                if actual >= BOOT_REPORT_SIZE {
                    let _ = session
                        .keyboard
                        .consume_report(&session.report[..BOOT_REPORT_SIZE]);
                    session.keyboard.drain_into_console();
                }
            }
            Err(UsbError::Nak | UsbError::Nyet) => session.naks = session.naks.saturating_add(1),
            Err(error) => {
                session.consecutive_errors = session.consecutive_errors.saturating_add(1);
                session.errors = session.errors.saturating_add(1);
                self.snapshot.errors = self.snapshot.errors.saturating_add(1);
                if session.consecutive_errors == 1 {
                    crate::usb_println!(
                        "USB: keyboard poll failed: {:?} (polls={}, reports={}, naks={}, errors={})",
                        error,
                        session.polls,
                        session.reports,
                        session.naks,
                        session.errors
                    );
                }
                if session.consecutive_errors >= 2 {
                    crate::usb_println!(
                        "USB: keyboard given up after {} consecutive {:?}                          (polls={}, reports={}, naks={}, errors={})",
                        session.consecutive_errors,
                        error,
                        session.polls,
                        session.reports,
                        session.naks,
                        session.errors
                    );
                    disconnected = session
                        .route
                        .translator
                        .map(|target| (target, session.route.device_speed));
                }
                if session.consecutive_errors >= 8 {
                    self.hid = None;
                    return;
                }
            }
        }
        if let Some((target, speed)) = disconnected {
            crate::usb_println!(
                "USB: keyboard disconnected from hub {} port {}",
                target.hub_address.get(),
                target.port_number
            );
            self.detach_hub_port(target.hub_address, target.port_number);
            self.fallback_probe = Some((target.hub_address, target.port_number, speed));
            if let Some(managed) = self.hub_ports.get_mut(target.port_number as usize - 1) {
                // GET_STATUS is unreliable on the Pi 3's LAN951x after a TT
                // transaction error. Queue a direct reset probe. While the
                // port is empty it is retried silently at the hub-monitor
                // interval; a reinserted device proceeds to enumeration.
                managed.connection_changed(true, now);
            }
            if self.hub_scan == HubScan::Idle {
                self.hub_scan = HubScan::Monitoring {
                    hub: target.hub_address,
                    ports: self.hub_ports.len() as u8,
                    next_scan_us: now.saturating_add(HUB_MONITOR_INTERVAL_US),
                };
            }
            return;
        }
        // Advance from the previous target rather than from the completion
        // time. A deadline of "completion + interval" always lands just after
        // a system tick, so the poll misses that tick and only happens on the
        // one after it: with a 10 ms endpoint on a 100 Hz tick the keyboard
        // ends up sampled every 20 ms, and a key tapped and released between
        // two samples is never seen at all. Clamping to `now` keeps a late
        // run from building up a backlog of polls.
        let interval_us = (session.endpoint.interval.max(1) as u64) * 1_000;
        session.next_poll_us = session.next_poll_us.saturating_add(interval_us).max(now);
    }
}

impl SystemService for UsbManager {
    fn poll(&mut self) -> Result<(), ServiceError> {
        self.poll_once();
        Ok(())
    }

    fn requires_continuous_polling(&self) -> bool {
        self.hcd.requires_foreground_polling()
    }
}

#[cfg(test)]
mod tests {
    use libusb::*;

    use super::*;
    use crate::io::usb::hcd::*;
    struct Fake {
        state: RootPortState,
        snapshot: HcdSnapshot,
    }
    impl HostController for Fake {
        fn root_port_state(&self) -> RootPortState {
            self.state
        }
        fn reset_root_port(&mut self, _: u64) -> Result<(), UsbError> {
            Ok(())
        }
        fn submit(&mut self, _: TransferRequest<'_>) -> Result<TransferToken, UsbError> {
            Err(UsbError::Unsupported)
        }
        fn cancel(&mut self, _: TransferToken) -> Result<TransferProgress, UsbError> {
            Ok(TransferProgress::Known(0))
        }
        fn reap(&mut self) -> Option<TransferCompletion> {
            None
        }
        fn service_timeouts(&mut self, _: u64) {}
        fn snapshot(&self) -> HcdSnapshot {
            self.snapshot
        }
    }
    fn now() -> u64 {
        0
    }
    #[test]
    fn address_reuse_gets_new_generation() {
        let mut m = UsbManager::new(
            Box::new(Fake {
                state: RootPortState::Disconnected,
                snapshot: HcdSnapshot::default(),
            }),
            now,
        );
        let a = m
            .allocate_device(UsbSpeed::Full, DeviceLocation::Root)
            .unwrap();
        let g = m.devices()[0].generation;
        m.detach(a);
        let b = m
            .allocate_device(UsbSpeed::Full, DeviceLocation::Root)
            .unwrap();
        assert!(m.devices()[0].generation > g);
        assert!(b.get() > 0)
    }
    #[test]
    fn resource_limit_is_bounded() {
        let mut m = UsbManager::new(
            Box::new(Fake {
                state: RootPortState::Disconnected,
                snapshot: HcdSnapshot::default(),
            }),
            now,
        );
        for p in 0..MAX_DEVICES {
            m.allocate_device(
                UsbSpeed::Full,
                DeviceLocation::Hub {
                    hub: UsbAddress::new(1).unwrap(),
                    port: p as u8,
                },
            )
            .unwrap();
        }
        assert_eq!(
            m.allocate_device(UsbSpeed::Full, DeviceLocation::Root),
            Err(UsbError::ResourceExhausted)
        )
    }
}

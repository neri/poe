//! Enumeration and input binding on top of [`Xhci`].
//!
//! Stage 4 of `docs/USB_HOST_RPI4_PLAN.md`.  This is the xHCI counterpart of
//! [`crate::io::usb::UsbManager`], and deliberately not the same object: the
//! existing manager is built around one root port, driver-issued SET_ADDRESS,
//! data toggles and split transactions, all of which an xHCI controller does
//! for itself.  Forcing one onto the other would mean faking those on this
//! side.  What is shared is everything above the controller — `libusb`
//! descriptors and requests, [`crate::io::usb::class`] and the console queue
//! in [`crate::io::usb::input`] — so a key from either stack is the same key.
//!
//! [`XhciUsb`] is a [`SystemService`]: each `poll` advances one port by one
//! step and returns.  Enumeration steps are control transfers, which this runs
//! synchronously because an xHCI controller completes one in microseconds when
//! the device answers; the deadline is what bounds a device that does not.
//! Only one device enumerates at a time, across both the root ports and the
//! hub, so only one control transfer is ever outstanding.
//!
//! Every Boot Protocol keyboard interface of every device feeds the console,
//! on a root port or behind a single tier of USB 2.0 hub.  One device can be
//! several interfaces — a keyboard with a second one for media keys, a
//! receiver that is keyboard and mouse at once, the Raspberry Pi 400's built-in
//! keyboard — so there is no meaningful "one keyboard" to pick, and each
//! interface gets its own key state.  A hub behind a hub and anything with no
//! Boot Protocol keyboard interface are enumerated and then left alone.
//!
//! SuperSpeed devices are driven on the USB3 root ports only.  On a Raspberry
//! Pi 4 or 400 that is exactly the blue sockets: the VL805's SuperSpeed lanes
//! go straight to its four USB3 root ports, while all USB 2.0 lanes share the
//! one USB 2.0 root port through the VL805's built-in hub.  A SuperSpeed hub
//! is enumerated and then left alone.

use alloc::boxed::Box;
use alloc::vec::Vec;

use libusb::{
    ConfigurationDescriptor, DescriptorIter, DeviceDescriptor, Direction, EndpointAddress,
    EndpointDescriptor, InterfaceDescriptor, SetupPacket, TransferType, UsbError, UsbSpeed,
};

use super::hub::{self, HubDescriptor};
use super::{BULK_BUFFER_BYTES, DeviceRoute, Xhci, XhciEnv, XhciSnapshot, context};
use crate::env::{ServiceError, SystemService};
use crate::io::usb::class::hid;
use crate::io::usb::class::hid_keyboard::{BOOT_REPORT_SIZE, BootKeyboard};
use crate::io::usb::class::msc::bot::{Pipe, Transfer};
use crate::io::usb::class::msc::registry::{self, Backend, DeviceInfo};
use crate::io::usb::class::msc::{self, MAX_INTERFACES_PER_DEVICE, MscInterface, MscSession};
use crate::usb_println;

/// Root ports this tracks.  The Raspberry Pi 4's VL805 has five: one USB 2.0
/// port carrying its built-in hub and four USB3 ports; QEMU's model has
/// eight.  A controller with more leaves the rest unmanaged rather than
/// growing this without bound.
pub const MAX_ROOT_PORTS: usize = 16;

/// How long a port is left to settle after a connection is first seen.
///
/// USB 2.0 asks for 100 ms of stable connection before the reset.  Below that
/// a device that is still being pushed into the socket enumerates from a
/// half-made contact and has to be thrown away again.
const DEBOUNCE_US: u64 = 150_000;

/// How long one enumeration step may take before the attempt is abandoned.
const STEP_TIMEOUT_US: u64 = 500_000;

/// How long to wait before retrying a port whose enumeration failed.
const RETRY_DELAY_US: u64 = 1_000_000;

/// How long between sweeps of a hub's downstream ports once they have all been
/// looked at.  A hub is polled rather than watched: using its status change
/// endpoint would mean a second interrupt endpoint per device, and a sweep
/// costs one control transfer per port at this interval.
const HUB_RESCAN_US: u64 = 250_000;

/// How many times a halted interrupt endpoint is recovered before the device
/// is dropped.  A keyboard that keeps stalling is broken or gone; retrying it
/// forever would cost a control transfer per poll and never converge.
const MAX_RECOVERIES: u8 = 3;

/// How many times a port is retried before it is left alone until the device
/// is unplugged.  Without a limit, a device that always fails the same way
/// costs a control transfer's worth of latency for as long as it stays in.
const MAX_ATTEMPTS: u8 = 3;

/// Keyboard interfaces that can feed the console at once, across all devices.
pub const MAX_KEYBOARDS: usize = 4;

/// Keyboard interfaces taken from one device.
const MAX_INTERFACES: usize = 4;

/// Largest configuration descriptor this reads.  A Boot Protocol keyboard's is
/// tens of bytes, a mass storage device's a few dozen; anything beyond this
/// is outside the scope of the plan.
const MAX_CONFIGURATION_BYTES: usize = 256;

/// Transfers a mass storage interface may start or finish in one poll.  Enough
/// to keep a read moving between system ticks, few enough that one interface
/// does not hold up the keyboards and the others.
const STORAGE_STEPS_PER_POLL: usize = 8;

/// What a root port is doing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PortState {
    /// Nothing connected, or a port this driver does not drive.
    Idle,
    /// A connection appeared; waiting for it to settle.
    Debouncing { until: u64 },
    /// Reset signalling is in progress.
    Resetting { deadline: u64 },
    /// Addressed and being interrogated.
    Enumerating {
        slot: u8,
        speed: UsbSpeed,
        step: Step,
    },
    /// Enumerated, and its keyboard interfaces are feeding the console.
    Bound { slot: u8 },
    /// A hub is here; its downstream ports are driven by [`HubState`].
    Hub { slot: u8 },
    /// Enumerated and not something this drives.  The slot is already released.
    Ignored,
    /// A keyboard that found the table full.  The slot is released; it is
    /// enumerated again when a keyboard leaves.
    WaitingForRoom,
    /// The attempt failed; waiting before trying again.
    Failed { retry_at: u64, attempts: u8 },
}

/// One control transfer's worth of enumeration, for a device on a root port or
/// behind the hub.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    /// The first eight bytes, which carry the real EP0 packet size.
    DeviceDescriptorHead,
    DeviceDescriptor,
    ConfigurationHead,
    Configuration {
        total: u16,
    },
    SetConfiguration {
        value: u8,
    },
    /// Per keyboard interface, in the order found: its endpoint, then Boot
    /// Protocol, then the idle rate.
    ConfigureEndpoint {
        index: u8,
    },
    SetProtocol {
        index: u8,
    },
    SetIdle {
        index: u8,
    },
    /// Per mass storage interface: its pair of bulk endpoints.
    ConfigureBulk {
        index: u8,
    },
    /// A hub has to have its configuration set before its ports answer.
    HubSetConfiguration {
        value: u8,
    },
    HubDescriptor,
}

/// A keyboard interface found in a configuration.
#[derive(Clone, Copy, Debug)]
struct KeyboardInterface {
    configuration: u8,
    interface: u8,
    endpoint: EndpointAddress,
    max_packet_size: u16,
    interval: u8,
    /// The device context index its endpoint was given, once configured.
    dci: u8,
}

/// One keyboard interface feeding the console.
struct Keyboard {
    at: Bound,
    slot: u8,
    dci: u8,
    interface: u8,
    keys: BootKeyboard,
    /// Consecutive failed transfers.  Only a good report clears it, so a
    /// device that fails, "recovers" and fails again is still counted — the
    /// recovery on its own proves nothing.
    errors: u8,
    /// The endpoint has halted and is waiting to be brought back.
    halted: bool,
}

/// A mass storage interface found in a configuration, and the device context
/// indexes its endpoints were given once configured.
#[derive(Clone, Copy, Debug)]
struct PendingStorage {
    interface: MscInterface,
    dci_in: u8,
    dci_out: u8,
}

/// One mass storage interface in use.  The session decides what to do; this
/// carries it out on the controller.
struct Storage {
    at: Bound,
    slot: u8,
    dci_in: u8,
    dci_out: u8,
    session: Box<MscSession>,
    /// The bulk transfer in flight, the endpoint it is on and its deadline.
    in_flight: Option<(u8, u64)>,
}

/// Where a device is attached.  Used both to remember where the console's
/// keyboard is and to say which port a diagnostic line is about, because a
/// root port 1 and a hub port 1 are different places.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Bound {
    /// Straight into a root port.
    Root(u8),
    /// On a downstream port of the hub.
    Hub(u8),
}

impl core::fmt::Display for Bound {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Root(port) => write!(f, "port {port}"),
            Self::Hub(port) => write!(f, "hub port {port}"),
        }
    }
}

/// The hub this driver is willing to drive, and what it is doing.
///
/// One at a time: the plan fixes the scope at a single tier, and a second hub
/// would need its own slot, ring and port cursor for no gain inside that
/// scope.  A second one is enumerated and then left alone.
struct HubState {
    root_port: u8,
    slot: u8,
    speed: UsbSpeed,
    ports: u8,
    /// The downstream port the cursor is on, 1-based.
    port: u8,
    phase: HubPhase,
    /// Ports whose device was enumerated and left alone, one bit per port
    /// (bit n = port n).  Remembered until that port reports a disconnect, so
    /// a device that is not a keyboard is interrogated once, not on every
    /// sweep.
    ignored: u16,
    /// The subset of `ignored` that was a keyboard turned away for want of
    /// room.  Only these are looked at again when a keyboard leaves; a mass
    /// storage device is not a keyboard however many times it is asked.
    waiting_for_room: u16,
    /// Consecutive failed enumerations per port.
    port_attempts: [u8; 16],
    /// Consecutive failures of requests to the hub itself.  Only these stop
    /// the scan: one bad device must not blind the other ports.
    hub_failures: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HubPhase {
    /// Switching each downstream port's power on in turn.
    Powering,
    /// Waiting for that power to be good.
    Settling { until: u64 },
    /// Reading the status of the port under the cursor.
    Scanning,
    /// Something is connected there; waiting for it to settle.
    Debouncing { until: u64 },
    /// Reset signalling was asked for; waiting for the hub to report it done.
    Resetting { deadline: u64 },
    /// The device behind the cursor port is being interrogated.
    Child {
        slot: u8,
        speed: UsbSpeed,
        step: Step,
    },
    /// Every port has been looked at; waiting before sweeping again.
    ///
    /// The sweep keeps running while a keyboard is bound: a hub reports a
    /// disconnect only when asked, and a device plugged into another port is
    /// only noticed by looking.
    Quiet { until: u64 },
    /// Given up on until the hub itself is unplugged.
    ///
    /// Its own state rather than `Quiet` with a far-off deadline: deadlines
    /// are compared as wrapping counters, so `u64::MAX` reads as already
    /// passed and the scan restarted at once, forever.
    Stopped,
}

/// Counters for the service itself, separate from the controller's.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeviceSnapshot {
    pub connects: u64,
    pub disconnects: u64,
    pub enumerated: u64,
    pub enumeration_failures: u64,
    pub keyboards_bound: u64,
    pub reports: u64,
    pub report_errors: u64,
    /// Halted endpoints brought back into service.
    pub recoveries: u64,
    /// Devices dropped because recovery did not take.
    pub recovery_failures: u64,
    pub ignored_devices: u64,
    /// Ports given up on after [`MAX_ATTEMPTS`].
    pub abandoned_ports: u64,
    pub hubs_configured: u64,
    pub hub_connects: u64,
    pub hub_disconnects: u64,
    pub storage_bound: u64,
}

/// Enumeration and input binding for one xHCI controller.
pub struct XhciUsb<'e, E: XhciEnv> {
    controller: Xhci<'e, E>,
    ports: [PortState; MAX_ROOT_PORTS],
    /// Ports this drives at all: USB 2.0 and USB3, within [`MAX_ROOT_PORTS`].
    managed: u16,
    /// The keyboard interfaces of the device currently being enumerated, and
    /// its class, which decides whether the hub path or the keyboard path
    /// runs once the configuration descriptor has been read.
    pending: [Option<KeyboardInterface>; MAX_INTERFACES],
    pending_storage: [Option<PendingStorage>; MAX_INTERFACES_PER_DEVICE],
    pending_class: u8,
    pending_ids: (u16, u16),
    keyboards: [Option<Keyboard>; MAX_KEYBOARDS],
    storage: Vec<Storage>,
    hub: Option<HubState>,
    /// Consecutive failed attempts per root port, cleared when the device is
    /// unplugged or the enumeration finally succeeds.
    attempts: [u8; MAX_ROOT_PORTS],
    snapshot: DeviceSnapshot,
}

impl<'e, E: XhciEnv> XhciUsb<'e, E> {
    /// Takes over a started controller and prepares its ports.
    pub fn new(mut controller: Xhci<'e, E>) -> Self {
        controller.power_ports();
        let mut managed = 0u16;
        let ports = controller.port_count().min(MAX_ROOT_PORTS as u8);
        for port in 1..=ports {
            // A port no Supported Protocol capability claims is left alone
            // rather than guessed at.
            let protocol = controller.port_protocol(port);
            if protocol.is_usb2() || protocol.is_usb3() {
                managed |= 1 << (port - 1);
            }
        }
        Self {
            controller,
            ports: [PortState::Idle; MAX_ROOT_PORTS],
            managed,
            pending: [None; MAX_INTERFACES],
            pending_storage: [None; MAX_INTERFACES_PER_DEVICE],
            pending_class: 0,
            pending_ids: (0, 0),
            keyboards: [const { None }; MAX_KEYBOARDS],
            storage: Vec::new(),
            hub: None,
            attempts: [0; MAX_ROOT_PORTS],
            snapshot: DeviceSnapshot::default(),
        }
    }

    pub fn controller_snapshot(&self) -> XhciSnapshot {
        self.controller.snapshot()
    }

    pub fn snapshot(&self) -> DeviceSnapshot {
        self.snapshot
    }

    /// True once at least one keyboard is delivering to the console.
    pub fn keyboard_ready(&self) -> bool {
        self.keyboards.iter().any(Option::is_some)
    }

    /// How many keyboard interfaces are feeding the console.
    pub fn keyboard_count(&self) -> usize {
        self.keyboards.iter().flatten().count()
    }

    /// The token an interrupt handler needs to silence this controller.
    pub fn interrupt_ack(&self) -> super::InterruptAck {
        self.controller.interrupt_ack()
    }

    /// Lets completions arrive as interrupts, so the CPU can sleep between
    /// them.  Call once something is draining the event ring — which, for
    /// this service, means after it has been registered or polled at least
    /// once.
    pub fn enable_interrupts(&mut self) {
        self.controller.enable_interrupts();
    }

    /// Gives up on interrupts and goes back to polling.  Used when the line
    /// turns out to be unserviceable rather than leaving the system wedged.
    pub fn disable_interrupts(&mut self) {
        self.controller.disable_interrupts();
    }

    /// Prints how much heap is free and how many entries the memory manager's
    /// map has, after a device's resources have been handed back.
    ///
    /// Both have to come back to the same values across plug cycles.  The map
    /// entries matter as much as the bytes: the manager does not merge freed
    /// neighbours, and running out of entries is an out-of-memory failure
    /// with plenty of memory free.
    fn report_memory(&self) {
        if cfg!(feature = "usb_debug") {
            usb_println!(
                "xHCI memory: {} KiB free, {} map entries, {} slot(s) kept after a failed Disable Slot",
                crate::mem::MemoryManager::free_memory_count() / 1024,
                crate::mem::MemoryManager::memory_list().count(),
                self.controller.snapshot().leaked_slots
            );
        }
    }

    /// The root ports this driver manages, as a bit per port (bit 0 = port 1).
    pub fn managed_ports(&self) -> u16 {
        self.managed
    }

    /// The managed root ports that are USB3, in the same form.
    pub fn usb3_ports(&self) -> u16 {
        (1..=MAX_ROOT_PORTS as u8)
            .filter(|&port| self.manages(port) && self.controller.is_usb3_port(port))
            .fold(0, |mask, port| mask | 1 << (port - 1))
    }

    /// True while some port is in a phase whose progress depends on time
    /// passing rather than on an event arriving.
    fn timing_sensitive(&self) -> bool {
        let root = self.ports.iter().any(|state| {
            matches!(
                state,
                PortState::Debouncing { .. }
                    | PortState::Resetting { .. }
                    | PortState::Enumerating { .. }
                    | PortState::Failed { .. }
            )
        });
        let recovering = self.keyboards.iter().flatten().any(|k| k.halted)
            || self
                .storage
                .iter()
                .any(|s| s.in_flight.is_some() || s.session.busy());
        // A hub is swept by polling, so it always wants foreground time —
        // except while it is quiet with a keyboard already running behind it,
        // which is the state a system with a hub spends its life in.
        let hub = self
            .hub
            .as_ref()
            .is_some_and(|hub| !matches!(hub.phase, HubPhase::Quiet { .. } | HubPhase::Stopped));
        root || hub || recovering
    }

    /// True while a device somewhere is between its reset and being bound
    /// or left alone.
    ///
    /// That whole stretch is serialised, not just the control transfers: the
    /// interfaces found on the device being enumerated are held in one place
    /// until it is bound, and two ports reset together would each enumerate
    /// over the other's.  It also keeps to one control transfer outstanding,
    /// which is what matching completions by TRB address relies on.
    fn enumerating(&self) -> bool {
        self.ports.iter().any(|state| {
            matches!(
                state,
                PortState::Resetting { .. } | PortState::Enumerating { .. }
            )
        }) || self.hub.as_ref().is_some_and(|hub| {
            matches!(
                hub.phase,
                HubPhase::Resetting { .. } | HubPhase::Child { .. }
            )
        })
    }

    #[inline]
    fn manages(&self, port: u8) -> bool {
        port >= 1 && port as usize <= MAX_ROOT_PORTS && self.managed & (1 << (port - 1)) != 0
    }

    #[inline]
    fn now(&self) -> u64 {
        self.controller.env().now_us()
    }

    /// True once `deadline` has passed, treating the counter as wrapping.
    #[inline]
    fn reached(&self, deadline: u64) -> bool {
        self.now().wrapping_sub(deadline) < i64::MAX as u64
    }

    /// Advances every port by at most one step.
    pub fn poll_once(&mut self) -> Result<(), UsbError> {
        self.controller.poll_events();
        self.controller.check_status()?;

        if self.controller.interrupts_enabled() && self.controller.env().interrupt_stalled() {
            // The platform took the line off as unserviceable.  Going back to
            // polling costs the CPU its idle time and nothing else, which is
            // a far better outcome than a keyboard that has silently stopped.
            self.controller.disable_interrupts();
            usb_println!("xHCI: interrupt unserviceable, falling back to polling");
        }

        // A Port Status Change event only says which port changed, so the
        // connect/disconnect edge is read from PORTSC either way.  Acting on
        // the register rather than the event also recovers a port whose event
        // was lost while the ring was full.
        //
        // The change bits of a managed port belong to its state machine and
        // are cleared at the edges it acts on — not here.  `PRC` in particular
        // is the handshake that says a reset finished, so acknowledging it
        // centrally would leave every reset looking unfinished forever.
        let changed = self.controller.take_port_changes();
        for port in 1..=MAX_ROOT_PORTS as u8 {
            if !self.manages(port) {
                // Nothing will ever look at this port's change bits, so clear
                // them rather than leave `USBSTS.PCD` latched.
                if changed & (1 << (port - 1)) != 0 && port <= self.controller.port_count() {
                    self.controller.acknowledge_port_change(port);
                }
                continue;
            }
            self.poll_port(port);
        }

        self.poll_hub();
        self.poll_keyboards();
        self.poll_storage();
        if !self.enumerating() {
            self.recover_one_keyboard();
        }
        Ok(())
    }

    fn poll_port(&mut self, port: u8) {
        let connected = self.controller.port_status(port).connected;
        let state = self.ports[port as usize - 1];

        if !connected {
            if state != PortState::Idle {
                self.detach_root(port);
            }
            return;
        }

        match state {
            PortState::Idle => {
                self.snapshot.connects += 1;
                self.controller.acknowledge_port_change(port);
                self.ports[port as usize - 1] = PortState::Debouncing {
                    until: self.now().wrapping_add(DEBOUNCE_US),
                };
            }
            PortState::Debouncing { until } => {
                if self.reached(until) && !self.enumerating() {
                    self.begin_reset(port);
                }
            }
            PortState::Resetting { deadline } => self.advance_reset(port, deadline),
            PortState::Enumerating { slot, speed, step } => {
                self.advance_enumeration(port, slot, speed, step)
            }
            PortState::Failed { retry_at, .. } => {
                if self.reached(retry_at) && !self.enumerating() {
                    self.begin_reset(port);
                }
            }
            PortState::Bound { .. }
            | PortState::Hub { .. }
            | PortState::Ignored
            | PortState::WaitingForRoom => {}
        }
    }

    fn begin_reset(&mut self, port: u8) {
        match self.controller.begin_port_reset(port) {
            Ok(()) => {
                self.ports[port as usize - 1] = PortState::Resetting {
                    deadline: self.now().wrapping_add(STEP_TIMEOUT_US),
                };
            }
            Err(_) => self.fail_root(port),
        }
    }

    fn advance_reset(&mut self, port: u8, deadline: u64) {
        match self.controller.poll_port_reset(port) {
            None => {
                if self.reached(deadline) {
                    if self.controller.is_usb3_port(port) {
                        let raw = self.controller.port_status(port).raw;
                        usb_println!(
                            "xHCI port {}: SuperSpeed link not up, PORTSC={:08x} (PLS {})",
                            port,
                            raw,
                            (raw >> 5) & 0xf
                        );
                    }
                    self.fail_root(port);
                }
            }
            Some(Err(_)) => self.fail_root(port),
            Some(Ok(speed)) => {
                let route = DeviceRoute {
                    root_port: port,
                    ..Default::default()
                };
                match self.address(route, speed) {
                    Ok(slot) => {
                        usb_println!(
                            "xHCI port {}: {:?} speed, slot {}, address {}",
                            port,
                            speed,
                            slot,
                            self.controller.device_address(slot).unwrap_or(0)
                        );
                        self.start_enumeration();
                        self.ports[port as usize - 1] = PortState::Enumerating {
                            slot,
                            speed,
                            step: Step::DeviceDescriptorHead,
                        };
                    }
                    Err(_) => self.fail_root(port),
                }
            }
        }
    }

    /// Enables a slot and addresses the device on `route`, cleaning up if the
    /// second half fails.
    fn address(&mut self, route: DeviceRoute, speed: UsbSpeed) -> Result<u8, UsbError> {
        let slot = self.controller.enable_slot()?;
        match self.controller.address_device(slot, route, speed) {
            Ok(()) => Ok(slot),
            Err(error) => {
                let _ = self.controller.disable_slot(slot);
                Err(error)
            }
        }
    }

    fn start_enumeration(&mut self) {
        self.pending = [None; MAX_INTERFACES];
        self.pending_storage = [None; MAX_INTERFACES_PER_DEVICE];
        self.pending_class = 0;
        self.pending_ids = (0, 0);
    }

    /// Moves the keyboard interfaces just configured on `slot` into the set
    /// that feeds the console.
    fn bind(&mut self, at: Bound, slot: u8) {
        self.bind_storage(at, slot);
        let pending = core::mem::replace(&mut self.pending, [None; MAX_INTERFACES]);
        if pending.iter().all(Option::is_none) {
            return;
        }
        for interface in pending.into_iter().flatten() {
            let Some(free) = self.keyboards.iter().position(Option::is_none) else {
                break;
            };
            self.keyboards[free] = Some(Keyboard {
                at,
                slot,
                dci: interface.dci,
                interface: interface.interface,
                keys: BootKeyboard::new(),
                errors: 0,
                halted: false,
            });
            self.snapshot.keyboards_bound += 1;
        }
        usb_println!(
            "xHCI {}: {} keyboard interface(s) now feed the console",
            at,
            self.keyboard_count()
        );
    }

    /// Publishes the mass storage interfaces just configured on `slot`.
    fn bind_storage(&mut self, at: Bound, slot: u8) {
        let pending =
            core::mem::replace(&mut self.pending_storage, [None; MAX_INTERFACES_PER_DEVICE]);
        let (vendor_id, product_id) = self.pending_ids;
        for found in pending.into_iter().flatten() {
            let mut info = DeviceInfo::new(
                Backend::Xhci,
                match at {
                    Bound::Root(port) => port,
                    Bound::Hub(_) => self.hub.as_ref().map_or(0, |hub| hub.root_port),
                },
                match at {
                    Bound::Root(_) => None,
                    Bound::Hub(port) => Some(port),
                },
                found.interface.number,
            );
            info.vendor_id = vendor_id;
            info.product_id = product_id;
            let Some(handle) = registry::with_global(|r| r.attach(info)) else {
                usb_println!(
                    "xHCI {}: mass storage interface {} refused, {} already in use",
                    at,
                    found.interface.number,
                    registry::MAX_DEVICES
                );
                continue;
            };
            usb_println!(
                "xHCI {}: mass storage interface {} is USB block device {}",
                at,
                found.interface.number,
                handle.index
            );
            self.snapshot.storage_bound += 1;
            self.storage.push(Storage {
                at,
                slot,
                dci_in: found.dci_in,
                dci_out: found.dci_out,
                session: Box::new(MscSession::new(
                    handle,
                    found.interface.number,
                    BULK_BUFFER_BYTES,
                )),
                in_flight: None,
            });
        }
    }

    /// Withdraws every mass storage interface of `slot`.  The slot is about
    /// to be disabled, which is what stops the controller using its buffers.
    fn unbind_storage(&mut self, slot: u8) {
        let mut index = 0;
        while index < self.storage.len() {
            if self.storage[index].slot == slot {
                let mut storage = self.storage.swap_remove(index);
                registry::with_global(|r| storage.session.detach(r));
            } else {
                index += 1;
            }
        }
    }

    /// Stops using every keyboard interface of `slot`, and every mass storage
    /// interface.  Returns true if there were keyboards.
    fn unbind(&mut self, slot: u8) -> bool {
        self.unbind_storage(slot);
        let mut any = false;
        for entry in self.keyboards.iter_mut() {
            if entry.as_ref().is_some_and(|k| k.slot == slot) {
                *entry = None;
                any = true;
            }
        }
        any
    }

    /// The slot of the device on hub port `port` whose keyboards or mass
    /// storage interfaces are bound.
    fn bound_on_hub_port(&self, port: u8) -> Option<u8> {
        self.keyboards
            .iter()
            .flatten()
            .find(|k| k.at == Bound::Hub(port))
            .map(|k| k.slot)
            .or_else(|| {
                self.storage
                    .iter()
                    .find(|s| s.at == Bound::Hub(port))
                    .map(|s| s.slot)
            })
    }

    fn advance_enumeration(&mut self, port: u8, slot: u8, speed: UsbSpeed, step: Step) {
        let next = match self.run_step(Bound::Root(port), slot, speed, step) {
            Ok(next) => next,
            Err(error) => {
                usb_println!("xHCI port {}: {:?} failed: {:?}", port, step, error);
                let _ = self.controller.disable_slot(slot);
                self.fail_root(port);
                return;
            }
        };
        match &next {
            Outcome::Continue(step) => {
                let step = *step;
                self.ports[port as usize - 1] = PortState::Enumerating { slot, speed, step }
            }
            Outcome::Bound => {
                self.snapshot.enumerated += 1;
                self.attempts[port as usize - 1] = 0;
                self.bind(Bound::Root(port), slot);
                self.ports[port as usize - 1] = PortState::Bound { slot };
            }
            Outcome::Hub(descriptor) => {
                let descriptor = *descriptor;
                self.snapshot.enumerated += 1;
                self.snapshot.hubs_configured += 1;
                self.attempts[port as usize - 1] = 0;
                self.hub = Some(HubState {
                    root_port: port,
                    slot,
                    speed,
                    ports: descriptor.ports,
                    port: 1,
                    phase: HubPhase::Powering,
                    ignored: 0,
                    waiting_for_room: 0,
                    port_attempts: [0; 16],
                    hub_failures: 0,
                });
                self.ports[port as usize - 1] = PortState::Hub { slot };
            }
            Outcome::NotForUs | Outcome::NoRoom => {
                self.snapshot.enumerated += 1;
                self.snapshot.ignored_devices += 1;
                self.attempts[port as usize - 1] = 0;
                let _ = self.controller.disable_slot(slot);
                self.ports[port as usize - 1] = if matches!(next, Outcome::NoRoom) {
                    PortState::WaitingForRoom
                } else {
                    PortState::Ignored
                };
                self.report_memory();
            }
        }
    }

    fn pending_interface(&self, index: u8) -> Result<KeyboardInterface, UsbError> {
        self.pending
            .get(index as usize)
            .copied()
            .flatten()
            .ok_or(UsbError::InvalidRequest)
    }

    /// Runs one enumeration step.  `at` is only used for the diagnostics.
    fn run_step(
        &mut self,
        at: Bound,
        slot: u8,
        speed: UsbSpeed,
        step: Step,
    ) -> Result<Outcome, UsbError> {
        match step {
            Step::DeviceDescriptorHead => {
                let mut head = [0u8; 8];
                self.control_in(
                    slot,
                    descriptor_request(libusb::DESCRIPTOR_DEVICE, 8),
                    &mut head,
                )?;
                // A Full Speed device may have been addressed with the wrong
                // EP0 size; its real one is only knowable from these bytes.
                // At SuperSpeed the byte is an exponent and always 512.
                let size =
                    context::ep0_packet_size(speed, head[7]).ok_or(UsbError::InvalidDescriptor)?;
                self.controller.set_max_packet_size(slot, size)?;
                Ok(Outcome::Continue(Step::DeviceDescriptor))
            }
            Step::DeviceDescriptor => {
                let mut bytes = [0u8; 18];
                self.control_in(
                    slot,
                    descriptor_request(libusb::DESCRIPTOR_DEVICE, 18),
                    &mut bytes,
                )?;
                let device = DeviceDescriptor::parse(&bytes)?;
                self.pending_ids = (device.vendor_id, device.product_id);
                usb_println!(
                    "xHCI {}: {:04x}:{:04x} USB {:04x} class {:02x}/{:02x}/{:02x} EP0 {}",
                    at,
                    device.vendor_id,
                    device.product_id,
                    device.usb,
                    device.class,
                    device.subclass,
                    device.protocol,
                    device.max_packet_size_0
                );
                if device.configurations == 0 {
                    return Ok(Outcome::NotForUs);
                }
                if device.class == hub::CLASS_HUB && speed == UsbSpeed::Super {
                    // Its descriptor, port status and Route String handling
                    // all differ from a USB 2.0 hub's.  Its USB 2.0 half, if
                    // any, is on the other bus and is handled there.
                    usb_println!("xHCI {}: SuperSpeed hub, not supported", at);
                    return Ok(Outcome::NotForUs);
                }
                self.pending_class = device.class;
                Ok(Outcome::Continue(Step::ConfigurationHead))
            }
            Step::ConfigurationHead => {
                let mut head = [0u8; 9];
                self.control_in(
                    slot,
                    descriptor_request(libusb::DESCRIPTOR_CONFIGURATION, 9),
                    &mut head,
                )?;
                let configuration = ConfigurationDescriptor::parse(&head)?;
                if self.pending_class == hub::CLASS_HUB {
                    // A hub's ports do not answer until its configuration is
                    // set, and its own descriptor is a class request rather
                    // than part of the configuration, so there is nothing more
                    // to read from the configuration first.
                    return Ok(Outcome::Continue(Step::HubSetConfiguration {
                        value: configuration.value,
                    }));
                }
                Ok(Outcome::Continue(Step::Configuration {
                    total: configuration.total_length,
                }))
            }
            Step::Configuration { total } => {
                let total = (total as usize).min(MAX_CONFIGURATION_BYTES);
                let mut bytes = [0u8; MAX_CONFIGURATION_BYTES];
                let read = self.control_in(
                    slot,
                    descriptor_request(libusb::DESCRIPTOR_CONFIGURATION, total as u16),
                    &mut bytes[..total],
                )?;
                let found = find_boot_keyboards(&bytes[..read])?;
                let storage = self.find_storage(at, &bytes[..read]);
                let count = found.iter().flatten().count();
                if count == 0 && storage == 0 {
                    return Ok(Outcome::NotForUs);
                }
                let room = self.keyboards.iter().filter(|k| k.is_none()).count();
                self.pending = [None; MAX_INTERFACES];
                if count > 0 && room == 0 {
                    usb_println!(
                        "xHCI {}: {} keyboard interface(s), but {} are already in use",
                        at,
                        count,
                        MAX_KEYBOARDS
                    );
                    if storage == 0 {
                        return Ok(Outcome::NoRoom);
                    }
                }
                for (slot_index, interface) in found.into_iter().flatten().take(room).enumerate() {
                    self.pending[slot_index] = Some(interface);
                }
                let value = self.pending[0]
                    .map(|k| k.configuration)
                    .or(self.pending_storage[0].map(|s| s.interface.configuration))
                    .unwrap_or(1);
                Ok(Outcome::Continue(Step::SetConfiguration { value }))
            }
            Step::SetConfiguration { value } => {
                self.control_out(slot, SetupPacket::set_configuration(value))?;
                Ok(self.after_keyboard(None))
            }
            Step::ConfigureEndpoint { index } => {
                let keyboard = self.pending_interface(index)?;
                let dci = self.controller.configure_interrupt_in(
                    slot,
                    keyboard.endpoint,
                    keyboard.max_packet_size,
                    keyboard.interval,
                )?;
                if let Some(entry) = self.pending[index as usize].as_mut() {
                    entry.dci = dci;
                }
                Ok(Outcome::Continue(Step::SetProtocol { index }))
            }
            Step::SetProtocol { index } => {
                let keyboard = self.pending_interface(index)?;
                self.control_out(slot, hid::set_protocol_boot(keyboard.interface))?;
                Ok(Outcome::Continue(Step::SetIdle { index }))
            }
            Step::SetIdle { index } => {
                let keyboard = self.pending_interface(index)?;
                // A non-zero idle makes the device restate the held keys, so a
                // lost report costs latency rather than a keystroke.  A device
                // that refuses SET_IDLE still works, so the result is ignored.
                let _ = self.control_out(
                    slot,
                    hid::set_idle(keyboard.interface, hid::IDLE_DURATION_4MS),
                );
                usb_println!(
                    "xHCI {}: boot keyboard on interface {} endpoint {:#04x}, {} bytes, bInterval {} ({:?})",
                    at,
                    keyboard.interface,
                    keyboard.endpoint.raw(),
                    keyboard.max_packet_size,
                    keyboard.interval,
                    speed
                );
                Ok(self.after_keyboard(Some(index)))
            }
            Step::ConfigureBulk { index } => {
                let pending = self
                    .pending_storage
                    .get(index as usize)
                    .copied()
                    .flatten()
                    .ok_or(UsbError::InvalidRequest)?;
                let interface = pending.interface;
                // A companion's burst means nothing below SuperSpeed.
                let burst = |b: u8| if speed == UsbSpeed::Super { b } else { 0 };
                let (dci_in, dci_out) = self.controller.configure_bulk_pair(
                    slot,
                    (
                        interface.bulk_in.address,
                        interface.bulk_in.max_packet_size,
                        burst(interface.burst_in),
                    ),
                    (
                        interface.bulk_out.address,
                        interface.bulk_out.max_packet_size,
                        burst(interface.burst_out),
                    ),
                )?;
                usb_println!(
                    "xHCI {}: mass storage interface {}, bulk {:#04x}/{:#04x}, {} bytes, burst {}/{} ({:?})",
                    at,
                    interface.number,
                    interface.bulk_in.address.raw(),
                    interface.bulk_out.address.raw(),
                    interface.bulk_in.max_packet_size,
                    burst(interface.burst_in),
                    burst(interface.burst_out),
                    speed
                );
                if let Some(entry) = self.pending_storage[index as usize].as_mut() {
                    entry.dci_in = dci_in;
                    entry.dci_out = dci_out;
                }
                let next = index + 1;
                if self
                    .pending_storage
                    .get(next as usize)
                    .is_some_and(Option::is_some)
                {
                    Ok(Outcome::Continue(Step::ConfigureBulk { index: next }))
                } else {
                    Ok(Outcome::Bound)
                }
            }
            Step::HubSetConfiguration { value } => {
                // Only one hub is driven.  A second one, or one behind the
                // first, is left configured but otherwise alone.
                if self.hub.is_some() {
                    return Ok(Outcome::NotForUs);
                }
                self.control_out(slot, SetupPacket::set_configuration(value))?;
                Ok(Outcome::Continue(Step::HubDescriptor))
            }
            Step::HubDescriptor => {
                let mut bytes = [0u8; 9];
                let read = self.control_in(slot, hub::get_hub_descriptor(9), &mut bytes)?;
                let descriptor = HubDescriptor::parse(&bytes[..read])?;
                // The controller has to know this slot is a hub before it will
                // route an Address Device through it.
                self.controller
                    .configure_hub(slot, descriptor.ports, descriptor.tt_think_time)?;
                usb_println!(
                    "xHCI {}: hub with {} port(s), TT think time {} ({:?})",
                    at,
                    descriptor.ports,
                    descriptor.tt_think_time,
                    speed
                );
                Ok(Outcome::Hub(descriptor))
            }
        }
    }

    /// What comes after keyboard interface `done` (or after SET_CONFIGURATION,
    /// when `None`): the next keyboard, then the mass storage interfaces.
    fn after_keyboard(&self, done: Option<u8>) -> Outcome {
        let next = done.map_or(0, |index| index + 1);
        if self.pending.get(next as usize).is_some_and(Option::is_some) {
            Outcome::Continue(Step::ConfigureEndpoint { index: next })
        } else if self.pending_storage[0].is_some() {
            Outcome::Continue(Step::ConfigureBulk { index: 0 })
        } else {
            Outcome::Bound
        }
    }

    /// Notes the mass storage interfaces of a configuration for binding, as
    /// many as there is room for.  Returns how many were taken.
    fn find_storage(&mut self, at: Bound, configuration: &[u8]) -> usize {
        self.pending_storage = [None; MAX_INTERFACES_PER_DEVICE];
        let found = match msc::find_interfaces(configuration) {
            Ok(found) => found,
            Err(error) => {
                usb_println!("xHCI {}: configuration descriptor: {:?}", at, error);
                return 0;
            }
        };
        for rejected in &found.rejected {
            usb_println!(
                "xHCI {}: mass storage interface {} not used: {}",
                at,
                rejected.number,
                rejected.reason
            );
        }
        let room = registry::with_global(|r| r.free_places());
        if found.interfaces.len() > room {
            usb_println!(
                "xHCI {}: {} mass storage interface(s) refused, {} already in use",
                at,
                found.interfaces.len() - room,
                registry::MAX_DEVICES
            );
            registry::with_global(|r| r.note_rejected(found.interfaces.len() - room));
        }
        let mut taken = 0;
        for interface in found.interfaces.into_iter().take(room) {
            self.pending_storage[taken] = Some(PendingStorage {
                interface,
                dci_in: 0,
                dci_out: 0,
            });
            taken += 1;
        }
        taken
    }

    // ---- the hub's downstream ports --------------------------------------

    fn poll_hub(&mut self) {
        let Some(hub) = self.hub.as_ref() else { return };
        let (root_port, slot, port, phase, ports) =
            (hub.root_port, hub.slot, hub.port, hub.phase, hub.ports);

        // The hub going away takes everything below it, and the root port
        // state machine is what notices that.
        if self.ports[root_port as usize - 1] != (PortState::Hub { slot }) {
            return;
        }

        match phase {
            HubPhase::Powering => {
                if self.enumerating() {
                    return;
                }
                if port > ports {
                    // The specification's settling time after power is good is
                    // what makes a device behind a freshly switched port
                    // believable; `HubDescriptor::parse` already floors it.
                    let until = self.now().wrapping_add(100_000);
                    self.set_hub(1, HubPhase::Settling { until });
                    return;
                }
                let _ =
                    self.control_out(slot, hub::set_port_feature(port, hub::feature::PORT_POWER));
                self.set_hub(port + 1, HubPhase::Powering);
            }
            HubPhase::Settling { until } => {
                if self.reached(until) {
                    self.set_hub(1, HubPhase::Scanning);
                }
            }
            HubPhase::Scanning => {
                if !self.enumerating() {
                    self.scan_hub_port(slot, port)
                }
            }
            HubPhase::Debouncing { until } => {
                if self.reached(until) && !self.enumerating() {
                    match self
                        .control_out(slot, hub::set_port_feature(port, hub::feature::PORT_RESET))
                    {
                        Ok(()) => {
                            let deadline = self.now().wrapping_add(STEP_TIMEOUT_US);
                            self.set_hub(port, HubPhase::Resetting { deadline })
                        }
                        Err(_) => self.hub_request_failed(),
                    }
                }
            }
            HubPhase::Resetting { deadline } => self.advance_hub_reset(slot, port, deadline),
            HubPhase::Child {
                slot: child,
                speed,
                step,
            } => self.advance_hub_child(child, speed, step),
            HubPhase::Quiet { until } => {
                if self.reached(until) {
                    self.set_hub(1, HubPhase::Scanning);
                }
            }
            HubPhase::Stopped => {}
        }
    }

    /// Replaces the hub's cursor and phase together, so no path updates one
    /// and forgets the other.
    fn set_hub(&mut self, port: u8, phase: HubPhase) {
        if let Some(hub) = self.hub.as_mut() {
            hub.port = port;
            hub.phase = phase;
        }
    }

    /// Moves the cursor to the next downstream port, or starts the wait before
    /// the next sweep.
    fn next_hub_port(&mut self) {
        let (port, ports) = match self.hub.as_ref() {
            Some(hub) => (hub.port, hub.ports),
            None => return,
        };
        if port >= ports {
            let until = self.now().wrapping_add(HUB_RESCAN_US);
            self.set_hub(1, HubPhase::Quiet { until });
        } else {
            self.set_hub(port + 1, HubPhase::Scanning);
        }
    }

    /// Reads one downstream port's status and decides what to do with it.
    ///
    /// This is the one place that issues more than one control transfer per
    /// poll: a change bit that is not acknowledged is reported again on the
    /// next sweep, and there are at most five of them.
    fn scan_hub_port(&mut self, slot: u8, port: u8) {
        let mut bytes = [0u8; 4];
        if self
            .control_in(slot, hub::get_port_status(port), &mut bytes)
            .is_err()
        {
            self.hub_request_failed();
            return;
        }
        let Ok(status) = hub::PortStatus::parse(&bytes) else {
            self.next_hub_port();
            return;
        };
        if let Some(hub) = self.hub.as_mut() {
            hub.hub_failures = 0;
        }
        for feature in status.pending_change_features() {
            let _ = self.control_out(slot, hub::clear_port_feature(port, feature));
        }

        let bound_here = self.bound_on_hub_port(port).is_some();
        if !status.connected {
            if bound_here {
                self.detach_hub_device(port);
            }
            self.forget_hub_port(port);
            self.next_hub_port();
            return;
        }
        if status.connection_changed {
            // Unplugged and plugged back in between two sweeps: whatever was
            // decided about the old device does not apply to the new one.
            if bound_here {
                self.detach_hub_device(port);
            }
            self.forget_hub_port(port);
        } else if bound_here || self.hub_port_ignored(port) {
            self.next_hub_port();
            return;
        }
        self.snapshot.hub_connects += 1;
        let until = self.now().wrapping_add(DEBOUNCE_US);
        self.set_hub(port, HubPhase::Debouncing { until });
    }

    fn hub_port_ignored(&self, port: u8) -> bool {
        self.hub
            .as_ref()
            .is_some_and(|hub| hub.ignored & (1 << port) != 0)
    }

    /// Clears what is remembered about a port whose device has gone.
    fn forget_hub_port(&mut self, port: u8) {
        if let Some(hub) = self.hub.as_mut() {
            hub.ignored &= !(1 << port);
            hub.waiting_for_room &= !(1 << port);
            hub.port_attempts[port as usize & 15] = 0;
        }
    }

    /// Takes the device on `port` off the bus.
    ///
    /// Done before its slot is released.  The controller hands a freed USB
    /// address straight to the next device, and a device left enabled keeps
    /// answering to it; behind a High Speed hub, which repeats High Speed
    /// traffic to every downstream port, it then answers for the next device
    /// too.  A mass storage device left this way broke every keyboard
    /// enumerated after it.
    fn disable_hub_port(&mut self, port: u8) {
        if let Some(slot) = self.hub.as_ref().map(|hub| hub.slot) {
            let _ = self.control_out(
                slot,
                hub::clear_port_feature(port, hub::feature::PORT_ENABLE),
            );
        }
    }

    /// Leaves the device on `port` alone until it is unplugged.
    fn ignore_hub_port(&mut self, port: u8, child: u8) {
        self.disable_hub_port(port);
        let _ = self.controller.disable_slot(child);
        if let Some(hub) = self.hub.as_mut() {
            hub.ignored |= 1 << port;
            hub.port_attempts[port as usize & 15] = 0;
        }
        usb_println!("xHCI hub port {}: left alone until unplugged", port);
        self.report_memory();
        self.next_hub_port();
    }

    fn advance_hub_reset(&mut self, slot: u8, port: u8, deadline: u64) {
        let mut bytes = [0u8; 4];
        if self
            .control_in(slot, hub::get_port_status(port), &mut bytes)
            .is_err()
        {
            self.hub_request_failed();
            return;
        }
        let Ok(status) = hub::PortStatus::parse(&bytes) else {
            self.fail_hub_port(port);
            return;
        };
        if !status.connected {
            self.next_hub_port();
            return;
        }
        if status.resetting || !status.reset_changed {
            if self.reached(deadline) {
                self.fail_hub_port(port);
            }
            return;
        }
        let _ = self.control_out(
            slot,
            hub::clear_port_feature(port, hub::feature::C_PORT_RESET),
        );
        let Some(speed) = status.speed else {
            self.fail_hub_port(port);
            return;
        };

        let Some(hub) = self.hub.as_ref() else { return };
        // A Low or Full Speed device behind a High Speed hub is reached
        // through that hub's transaction translator, and the controller is
        // what schedules the split transactions — so it has to be told which
        // translator, and on which of the hub's ports.
        let split = hub.speed == UsbSpeed::High && speed != UsbSpeed::High;
        let route = DeviceRoute {
            root_port: hub.root_port,
            route_string: hub::route_string_for(port),
            tt_hub_slot: if split { hub.slot } else { 0 },
            tt_port: if split { port } else { 0 },
            // Think Time belongs to the hub's own Slot Context; for a device
            // behind it the field is reserved.  Linux leaves it zero too.
            tt_think_time: 0,
        };
        match self.address(route, speed) {
            Err(error) => {
                let snapshot = self.controller.snapshot();
                usb_println!(
                    "xHCI hub port {}: {:?} speed, Address Device failed: {:?} \
                     (TRB type {} completion code {}){}",
                    port,
                    speed,
                    error,
                    snapshot.last_failed_command,
                    snapshot.last_failed_completion,
                    if split {
                        ", through the hub's translator"
                    } else {
                        ""
                    }
                );
                self.fail_hub_port(port);
            }
            Ok(child) => {
                usb_println!(
                    "xHCI hub port {}: {:?} speed, slot {}, address {}{}",
                    port,
                    speed,
                    child,
                    self.controller.device_address(child).unwrap_or(0),
                    if split {
                        ", through the hub's translator"
                    } else {
                        ""
                    }
                );
                self.start_enumeration();
                self.set_hub(
                    port,
                    HubPhase::Child {
                        slot: child,
                        speed,
                        step: Step::DeviceDescriptorHead,
                    },
                );
            }
        }
    }

    fn advance_hub_child(&mut self, child: u8, speed: UsbSpeed, step: Step) {
        let port = self.hub.as_ref().map(|hub| hub.port).unwrap_or(0);
        let next = match self.run_step(Bound::Hub(port), child, speed, step) {
            Ok(next) => next,
            Err(error) => {
                usb_println!("xHCI hub port {}: enumeration failed: {:?}", port, error);
                let _ = self.controller.disable_slot(child);
                self.fail_hub_port(port);
                return;
            }
        };
        match next {
            Outcome::Continue(step) => self.set_hub(
                port,
                HubPhase::Child {
                    slot: child,
                    speed,
                    step,
                },
            ),
            Outcome::Bound => {
                self.snapshot.enumerated += 1;
                if let Some(hub) = self.hub.as_mut() {
                    hub.port_attempts[port as usize & 15] = 0;
                }
                self.bind(Bound::Hub(port), child);
                // Carry on sweeping: the rest of the ports still need looking
                // at, and this one is checked for a disconnect each time round.
                self.next_hub_port();
            }
            // A hub behind the hub is outside the plan's scope, and so is
            // anything that is not a keyboard.
            Outcome::Hub(_) | Outcome::NotForUs => {
                self.snapshot.enumerated += 1;
                self.snapshot.ignored_devices += 1;
                self.ignore_hub_port(port, child);
            }
            Outcome::NoRoom => {
                self.snapshot.enumerated += 1;
                self.snapshot.ignored_devices += 1;
                if let Some(hub) = self.hub.as_mut() {
                    hub.waiting_for_room |= 1 << port;
                }
                self.ignore_hub_port(port, child);
            }
        }
    }

    /// Counts a failed enumeration on `port`, and gives up on that port —
    /// not the hub — after [`MAX_ATTEMPTS`].
    fn fail_hub_port(&mut self, port: u8) {
        self.snapshot.enumeration_failures += 1;
        self.disable_hub_port(port);
        let attempts = match self.hub.as_mut() {
            Some(hub) => {
                let slot = &mut hub.port_attempts[port as usize & 15];
                *slot = slot.saturating_add(1);
                *slot
            }
            None => return,
        };
        if attempts >= MAX_ATTEMPTS {
            self.snapshot.abandoned_ports += 1;
            let snapshot = self.controller.snapshot();
            usb_println!(
                "xHCI hub port {}: giving up after {} attempts until unplugged \
                 (last failed command: TRB type {} completion code {})",
                port,
                attempts,
                snapshot.last_failed_command,
                snapshot.last_failed_completion
            );
            if let Some(hub) = self.hub.as_mut() {
                hub.ignored |= 1 << port;
            }
        }
        self.next_hub_port();
    }

    /// Counts a request to the hub itself that failed.  After
    /// [`MAX_ATTEMPTS`] in a row the hub is not answering and the scan stops
    /// until it is unplugged.
    fn hub_request_failed(&mut self) {
        let failures = match self.hub.as_mut() {
            Some(hub) => {
                hub.hub_failures = hub.hub_failures.saturating_add(1);
                hub.hub_failures
            }
            None => return,
        };
        if failures >= MAX_ATTEMPTS {
            self.snapshot.abandoned_ports += 1;
            usb_println!("xHCI: the hub stopped answering, stopping its port scan");
            if let Some(hub) = self.hub.as_mut() {
                hub.phase = HubPhase::Stopped;
            }
            return;
        }
        self.next_hub_port();
    }

    /// Releases the keyboard device on hub port `port`.
    fn detach_hub_device(&mut self, port: u8) {
        if let Some(slot) = self.bound_on_hub_port(port) {
            let keyboards = self.unbind(slot);
            let _ = self.controller.disable_slot(slot);
            self.snapshot.hub_disconnects += 1;
            if keyboards {
                usb_println!("xHCI hub port {}: keyboard detached", port);
                self.room_freed();
            } else {
                usb_println!("xHCI hub port {}: device detached", port);
                self.report_memory();
            }
        }
    }

    /// A keyboard has left: anything turned away for want of room gets
    /// another look, and nothing else does.
    fn room_freed(&mut self) {
        if let Some(hub) = self.hub.as_mut() {
            hub.ignored &= !hub.waiting_for_room;
            hub.waiting_for_room = 0;
        }
        for state in self.ports.iter_mut() {
            if *state == PortState::WaitingForRoom {
                // Back through reset and enumeration, as if just plugged in.
                *state = PortState::Idle;
            }
        }
        self.report_memory();
    }

    /// Releases every keyboard device behind the hub — the hub itself has gone.
    fn detach_all_hub_devices(&mut self) {
        for port in 1..=hub::MAX_DOWNSTREAM_PORTS {
            self.detach_hub_device(port);
        }
    }

    // ---- the keyboards ------------------------------------------------------

    /// Keeps a transfer queued on every keyboard interface and hands what
    /// comes back to the console.
    fn poll_keyboards(&mut self) {
        for index in 0..MAX_KEYBOARDS {
            let Some(keyboard) = self.keyboards[index].as_ref() else {
                continue;
            };
            if keyboard.halted {
                continue;
            }
            let (slot, dci) = (keyboard.slot, keyboard.dci);
            if self.controller.submit_interrupt_in(slot, dci).is_err() {
                // The slot or the endpoint is gone from under it.
                self.drop_keyboard_device(slot, Gone::Yes);
                continue;
            }
            let mut report = [0u8; BOOT_REPORT_SIZE];
            let result = self.controller.poll_interrupt_in(slot, dci, &mut report);
            let Some(keyboard) = self.keyboards[index].as_mut() else {
                continue;
            };
            match result {
                None => {}
                Some(Ok(length)) => {
                    self.snapshot.reports += 1;
                    keyboard.errors = 0;
                    if length == BOOT_REPORT_SIZE {
                        let _ = keyboard.keys.consume_report(&report);
                        keyboard.keys.drain_into_console();
                    }
                }
                Some(Err(UsbError::Disconnected)) => self.drop_keyboard_device(slot, Gone::Yes),
                Some(Err(_)) => {
                    // A Stall, a run of transaction errors, or the device
                    // being unplugged behind a hub — which looks the same from
                    // here.  Recovery finds out which.
                    self.snapshot.report_errors += 1;
                    keyboard.errors = keyboard.errors.saturating_add(1);
                    keyboard.halted = true;
                }
            }
        }
    }

    /// Brings one halted keyboard endpoint back, or drops its device.
    ///
    /// The device is checked for first.  Behind a hub the controller keeps a
    /// slot valid after its device is unplugged, so the host half of recovery
    /// succeeds regardless; only the port status says the device has gone.
    fn recover_one_keyboard(&mut self) {
        let Some(index) = self
            .keyboards
            .iter()
            .position(|k| k.as_ref().is_some_and(|k| k.halted))
        else {
            return;
        };
        let Some(keyboard) = self.keyboards[index].as_ref() else {
            return;
        };
        let (at, slot, dci, errors) = (keyboard.at, keyboard.slot, keyboard.dci, keyboard.errors);

        if !self.device_present(at) {
            self.drop_keyboard_device(slot, Gone::Yes);
            return;
        }
        if errors > MAX_RECOVERIES {
            self.snapshot.recovery_failures += 1;
            usb_println!("xHCI {}: keyboard keeps failing, dropping it", at);
            self.drop_keyboard_device(slot, Gone::No);
            return;
        }
        let recovered = self.controller.recover_interrupt_in(slot, dci).is_ok()
            && match self.controller.interrupt_in_endpoint(slot, dci) {
                // The device still thinks the endpoint is halted until told
                // otherwise, and a device that cannot be told is not back.
                Some(endpoint) => self
                    .control_out(slot, clear_endpoint_halt(endpoint))
                    .is_ok(),
                None => false,
            };
        let Some(keyboard) = self.keyboards[index].as_mut() else {
            return;
        };
        if recovered {
            keyboard.halted = false;
            self.snapshot.recoveries += 1;
            usb_println!(
                "xHCI {}: keyboard interface {} recovered",
                at,
                keyboard.interface
            );
        } else {
            keyboard.errors = keyboard.errors.saturating_add(1);
        }
    }

    /// Whether whatever is attached at `at` is still connected.
    fn device_present(&mut self, at: Bound) -> bool {
        match at {
            Bound::Root(port) => self.controller.port_status(port).connected,
            Bound::Hub(port) => {
                let Some(slot) = self.hub.as_ref().map(|hub| hub.slot) else {
                    return false;
                };
                let mut bytes = [0u8; 4];
                self.control_in(slot, hub::get_port_status(port), &mut bytes)
                    .ok()
                    .and_then(|_| hub::PortStatus::parse(&bytes).ok())
                    .is_some_and(|status| status.connected)
            }
        }
    }

    /// Stops using every keyboard interface of `slot` and releases the device.
    ///
    /// A device that has gone leaves its port to notice the next connection.
    /// One that is still there but keeps failing is taken off the bus and left
    /// alone until it is unplugged, rather than enumerated and dropped in a
    /// loop.
    fn drop_keyboard_device(&mut self, slot: u8, gone: Gone) {
        let Some(at) = self
            .keyboards
            .iter()
            .flatten()
            .find(|k| k.slot == slot)
            .map(|k| k.at)
        else {
            return;
        };
        self.unbind(slot);
        match at {
            Bound::Root(port) => {
                let _ = self.controller.disable_slot(slot);
                self.ports[port as usize - 1] = match gone {
                    Gone::Yes => PortState::Idle,
                    Gone::No => PortState::Ignored,
                };
                usb_println!("xHCI port {}: keyboard detached", port);
            }
            Bound::Hub(port) => {
                if gone == Gone::No {
                    self.disable_hub_port(port);
                }
                let _ = self.controller.disable_slot(slot);
                self.snapshot.hub_disconnects += 1;
                usb_println!("xHCI hub port {}: keyboard detached", port);
                if let Some(hub) = self.hub.as_mut() {
                    match gone {
                        Gone::Yes => {
                            hub.ignored &= !(1 << port);
                            hub.port_attempts[port as usize & 15] = 0;
                        }
                        Gone::No => hub.ignored |= 1 << port,
                    }
                }
            }
        }
        self.room_freed();
    }

    // ---- mass storage -----------------------------------------------------

    /// Carries out what each mass storage session asks for, a bounded number
    /// of transfers per interface per poll.
    fn poll_storage(&mut self) {
        for index in 0..self.storage.len() {
            for _ in 0..STORAGE_STEPS_PER_POLL {
                if !self.step_storage(index) {
                    break;
                }
            }
        }
    }

    /// Advances one interface by one transfer.  Returns false when there is
    /// nothing more to do for it in this poll.
    fn step_storage(&mut self, index: usize) -> bool {
        let now = self.now();
        let Self {
            controller,
            storage,
            ..
        } = self;
        let Some(storage) = storage.get_mut(index) else {
            return false;
        };
        registry::with_global(|r| storage.session.poll(now, r));
        let slot = storage.slot;
        if let Some((dci, deadline)) = storage.in_flight {
            let result = match controller.poll_bulk(slot, dci, storage.session.in_buffer()) {
                Some(result) => result,
                None if now.wrapping_sub(deadline) < i64::MAX as u64 => {
                    // The session's deadline has passed.  The transfer is
                    // stopped before it is reported, so its buffer is free
                    // again; if it cannot be stopped the endpoint is
                    // quarantined and every later transfer on it fails.
                    let _ = controller.cancel_bulk(slot, dci);
                    Err(UsbError::Timeout)
                }
                None => return false,
            };
            storage.in_flight = None;
            registry::with_global(|r| storage.session.complete(result, now, r));
            return true;
        }
        let Some(transfer) = storage.session.wanted() else {
            return false;
        };
        let deadline = storage.session.start(now);
        let result = match transfer {
            Transfer::BulkOut { len } => {
                let dci = storage.dci_out;
                match controller.submit_bulk(slot, dci, len, storage.session.out_data()) {
                    Ok(()) => {
                        storage.in_flight = Some((dci, deadline));
                        return true;
                    }
                    Err(error) => Err(error),
                }
            }
            Transfer::BulkIn { len } => {
                let dci = storage.dci_in;
                match controller.submit_bulk(slot, dci, len, &[]) {
                    Ok(()) => {
                        storage.in_flight = Some((dci, deadline));
                        return true;
                    }
                    Err(error) => Err(error),
                }
            }
            // Control transfers run to completion here, bounded by the
            // controller's own one-second wait, the same as enumeration's.
            Transfer::Control { setup } => controller.control_transfer(
                slot,
                setup.to_bytes(),
                storage.session.in_buffer(),
                setup.direction(),
            ),
            Transfer::ClearHalt { pipe } => {
                let dci = match pipe {
                    Pipe::In => storage.dci_in,
                    Pipe::Out => storage.dci_out,
                };
                controller.reset_bulk(slot, dci).and_then(|()| {
                    let endpoint = controller
                        .bulk_endpoint_address(slot, dci)
                        .ok_or(UsbError::InvalidRequest)?;
                    controller.control_transfer(
                        slot,
                        msc::wire::clear_endpoint_halt(endpoint).to_bytes(),
                        &mut [],
                        Direction::Out,
                    )
                })
            }
        };
        registry::with_global(|r| storage.session.complete(result, now, r));
        true
    }

    // ---- teardown --------------------------------------------------------

    /// Releases everything held for a root port and returns it to `Idle`.
    fn detach_root(&mut self, port: u8) {
        self.controller.acknowledge_port_change(port);
        let state = core::mem::replace(&mut self.ports[port as usize - 1], PortState::Idle);
        // A hub goes away with everything behind it.  Its child's slot is
        // released first: disabling the hub's own slot invalidates the route
        // that reaches the child, and afterwards there would be no way to
        // stop the controller walking the child's rings.
        if matches!(state, PortState::Hub { .. })
            && self.hub.as_ref().is_some_and(|hub| hub.root_port == port)
        {
            self.detach_all_hub_devices();
            self.hub = None;
        }
        let freed = matches!(state, PortState::Bound { slot } if self.unbind(slot));
        if freed {
            usb_println!("xHCI port {}: keyboard detached", port);
        } else if matches!(state, PortState::Bound { .. }) {
            usb_println!("xHCI port {}: device detached", port);
        }
        match state {
            PortState::Enumerating { slot, .. }
            | PortState::Bound { slot }
            | PortState::Hub { slot } => {
                // Disable Slot is what stops the controller walking this
                // device's rings; `disable_slot` only frees them once it has
                // completed.
                let _ = self.controller.disable_slot(slot);
            }
            _ => {}
        }
        self.attempts[port as usize - 1] = 0;
        if state != PortState::Idle {
            self.snapshot.disconnects += 1;
        }
        self.pending = [None; MAX_INTERFACES];
        self.pending_storage = [None; MAX_INTERFACES_PER_DEVICE];
        if freed {
            self.room_freed();
        } else if state != PortState::Idle {
            self.report_memory();
        }
    }

    fn fail_root(&mut self, port: u8) {
        let attempts = self.attempts[port as usize - 1].saturating_add(1);
        self.attempts[port as usize - 1] = attempts;
        self.snapshot.enumeration_failures += 1;
        self.pending = [None; MAX_INTERFACES];
        if attempts >= MAX_ATTEMPTS {
            self.snapshot.abandoned_ports += 1;
            usb_println!("xHCI port {}: giving up after {} attempts", port, attempts);
            // `Ignored` rather than `Idle`: the port keeps its state until the
            // device is unplugged, which is the edge that clears it.
            self.ports[port as usize - 1] = PortState::Ignored;
            return;
        }
        self.ports[port as usize - 1] = PortState::Failed {
            retry_at: self.now().wrapping_add(RETRY_DELAY_US),
            attempts,
        };
    }

    fn control_in(
        &mut self,
        slot: u8,
        setup: SetupPacket,
        buffer: &mut [u8],
    ) -> Result<usize, UsbError> {
        self.controller
            .control_transfer(slot, setup.to_bytes(), buffer, Direction::In)
    }

    fn control_out(&mut self, slot: u8, setup: SetupPacket) -> Result<(), UsbError> {
        self.controller
            .control_transfer(slot, setup.to_bytes(), &mut [], Direction::Out)
            .map(|_| ())
    }
}

/// Whether a keyboard being dropped has actually been unplugged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Gone {
    Yes,
    No,
}

/// What a completed enumeration step decided.
enum Outcome {
    Continue(Step),
    Bound,
    Hub(HubDescriptor),
    NotForUs,
    /// A keyboard, but every place in the table is taken.  Unlike
    /// `NotForUs`, worth looking at again once a keyboard leaves.
    NoRoom,
}

const fn descriptor_request(kind: u8, length: u16) -> SetupPacket {
    SetupPacket::get_descriptor(kind, 0, 0, length)
}

/// CLEAR_FEATURE(ENDPOINT_HALT) for one endpoint: request type 0x02 is
/// host-to-device, standard, endpoint; feature selector 0 is ENDPOINT_HALT.
const fn clear_endpoint_halt(endpoint: EndpointAddress) -> SetupPacket {
    SetupPacket::new(0x02, 0x01, 0, endpoint.raw() as u16, 0)
}

/// Finds every Boot Protocol keyboard interface and its interrupt IN
/// endpoint, in the order the configuration lists them.
///
/// Only alternate setting 0 counts: an interface's other settings are
/// alternatives to it, not more interfaces, and a device starts in setting 0.
fn find_boot_keyboards(
    bytes: &[u8],
) -> Result<[Option<KeyboardInterface>; MAX_INTERFACES], UsbError> {
    let configuration = ConfigurationDescriptor::parse(bytes.get(..9).unwrap_or(&[]))?;
    let mut found = [None; MAX_INTERFACES];
    let mut count = 0;
    let mut interface: Option<InterfaceDescriptor> = None;
    for item in DescriptorIter::new(bytes) {
        let item = item?;
        match item.descriptor_type {
            libusb::DESCRIPTOR_INTERFACE => {
                interface = InterfaceDescriptor::parse(item.bytes).ok().filter(|i| {
                    i.alternate == 0
                        && i.class == hid::CLASS_HID
                        && i.subclass == hid::SUBCLASS_BOOT
                        && i.protocol == hid::PROTOCOL_KEYBOARD
                });
            }
            libusb::DESCRIPTOR_ENDPOINT => {
                let Some(current) = interface else { continue };
                let Ok(endpoint) = EndpointDescriptor::parse(item.bytes) else {
                    continue;
                };
                if endpoint.transfer_type != TransferType::Interrupt
                    || endpoint.address.direction() != Direction::In
                {
                    continue;
                }
                if count < MAX_INTERFACES {
                    found[count] = Some(KeyboardInterface {
                        configuration: configuration.value,
                        interface: current.number,
                        endpoint: endpoint.address,
                        max_packet_size: endpoint.max_packet_size,
                        interval: endpoint.interval,
                        dci: 0,
                    });
                    count += 1;
                }
                // One endpoint per interface: the next endpoint of the same
                // interface, if any, is not another keyboard.
                interface = None;
            }
            _ => {}
        }
    }
    Ok(found)
}

impl<E: XhciEnv> SystemService for XhciUsb<'_, E> {
    fn poll(&mut self) -> Result<(), ServiceError> {
        self.poll_once().map_err(|_| ServiceError::Failed)
    }

    fn requires_continuous_polling(&self) -> bool {
        // Without an interrupt nothing would wake the system to notice a
        // completion.  With one, the steady state — a bound keyboard, or
        // nothing plugged in — is woken by the controller, and only the
        // timed phases of enumeration still want foreground time.
        !self.controller.interrupts_enabled() || self.timing_sensitive()
    }
}

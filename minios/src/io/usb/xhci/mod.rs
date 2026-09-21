//! A platform-independent xHCI host controller driver.
//!
//! See `docs/USB_HOST_RPI4_PLAN.md`.  This is the common half: it knows the
//! register interface, the rings and the context model, and nothing about how
//! the controller is attached.  A board supplies MMIO, DMA and time through
//! [`XhciEnv`]; the PCI glue for the QEMU virt machine lives in
//! `platform::arm64dt::xhci_pci`.
//!
//! What works today: reset and start, the command and event rings, root port
//! discovery and reset, slot allocation, Address Device, control transfers and
//! interrupt IN endpoints.  [`device::XhciUsb`] drives enumeration on top of
//! it and feeds the console.  Devices behind a hub are not handled yet: they
//! need the Route String and the transaction translator fields.
//!
//! Completions are polled.  Every wait takes a deadline, so a controller that
//! stops answering costs a bounded amount of time and never wedges the boot.
//!
//! Transfer Events are filed against the endpoint that produced them, so an
//! interrupt completion arriving while a control transfer is in flight is kept
//! rather than discarded.  [`Xhci::control_transfer`] still blocks until its
//! own event arrives, so only one control transfer may be outstanding at a
//! time; [`device::XhciUsb`] enforces that by enumerating one port at a time.

use alloc::boxed::Box;
use alloc::vec::Vec;

use libusb::{Direction, EndpointAddress, TransferType, UsbError, UsbSpeed};

pub mod context;
pub mod device;
pub mod env;
pub mod hub;
pub mod regs;
pub mod trb;

use context::{ContextLayout, EndpointContextFields, SlotContextFields};
pub use env::{Dma, DmaAllocation, XhciEnv};
use regs::{Capabilities, PortProtocol, Regs};
use trb::{ErstEntry, EventRingState, RingState, Trb, trb_flags, trb_type};

/// TRBs per command, transfer and event ring segment.  One 4 KiB page each,
/// which is also the granularity the allocator hands out.
const RING_TRBS: usize = 256;

/// Slots this driver is willing to manage at once.  The plan fixes the scope
/// at one keyboard behind at most one hub, so a controller advertising 255
/// slots still costs a bounded amount of memory here.
const MAX_SLOTS: u8 = 8;

/// Bytes of the shared bounce buffer used for control transfer data.
const BOUNCE_BYTES: usize = 512;

/// Interrupt IN endpoints one device may have running at once.
const MAX_INTERRUPT_ENDPOINTS: usize = 4;

/// Bytes reserved per interrupt IN endpoint.  A Boot Protocol keyboard report
/// is eight; the rest is headroom for a device that declares a larger packet.
const INTERRUPT_BUFFER_BYTES: usize = 64;

/// How long a controller has to finish a reset or come out of `CNR`.
const RESET_TIMEOUT_US: u64 = 1_000_000;

/// How long a command or transfer may take before it is given up on.
const COMMAND_TIMEOUT_US: u64 = 1_000_000;

/// Where a device sits on the bus, as the controller needs it described.
///
/// A device on a root port needs only `root_port`.  One behind a hub also
/// needs the Route String, and — if it is Low or Full Speed behind a High
/// Speed hub — the transaction translator that hub provides, because the
/// controller is what schedules the split transactions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeviceRoute {
    /// 1-based root hub port the whole chain hangs off.
    pub root_port: u8,
    /// Four bits per tier, first tier in the low nibble.  Zero for a
    /// device plugged straight into a root port.
    pub route_string: u32,
    /// Slot ID of the High Speed hub providing the translator, or 0.
    pub tt_hub_slot: u8,
    /// The downstream port of that hub this device is on.
    pub tt_port: u8,
    /// `TT Think Time` from that hub's descriptor.
    pub tt_think_time: u8,
}

/// A root hub port, as this driver sees it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortStatus {
    pub connected: bool,
    pub enabled: bool,
    pub resetting: bool,
    pub powered: bool,
    /// `None` for a link the driver does not drive, such as SuperSpeed.
    pub speed: Option<UsbSpeed>,
    pub protocol: PortProtocol,
    pub raw: u32,
}

/// Counters for diagnostics.  Cheap to keep and the only way to tell a quiet
/// controller from a stuck one.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct XhciSnapshot {
    pub commands: u64,
    pub command_errors: u64,
    pub command_timeouts: u64,
    pub transfers: u64,
    pub transfer_errors: u64,
    pub transfer_timeouts: u64,
    pub port_events: u64,
    pub events: u64,
    pub unexpected_events: u64,
    pub dropped_events: u64,
    /// Interrupt completions overwritten before the caller claimed them.
    pub dropped_reports: u64,
    pub host_errors: u64,
    /// Slots whose Disable Slot command did not complete, so their contexts
    /// and rings were kept rather than freed under a live DMA.
    pub leaked_slots: u64,
    /// Halted endpoints reset and re-pointed rather than abandoned.
    pub endpoint_recoveries: u64,
    /// The TRB type of the last command that failed, and the completion code
    /// it failed with.  The mapped `UsbError` loses the distinction between,
    /// say, a Parameter Error and a Context State Error, which is exactly what
    /// a failure on new hardware needs.
    pub last_failed_command: u8,
    pub last_failed_completion: u8,
    /// The `USBSTS` value that last looked like a fault, kept for diagnosis.
    pub last_fault_status: u32,
}

/// The two registers an interrupt handler has to write to stop the controller
/// asserting, and nothing else.
///
/// A handler runs without a `&mut Xhci` — there is no way to borrow one from
/// an ISR — so it gets this instead: a `Copy` token a platform can stash in a
/// static.  Acknowledging also masks the interrupter; [`Xhci::poll_events`]
/// re-arms it once the ring has been drained, which is what keeps a
/// level-triggered line from being re-taken before the foreground has run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterruptAck {
    usbsts: usize,
    iman: usize,
}

impl InterruptAck {
    /// Rebuilds a token from addresses a platform stashed away.
    ///
    /// # Safety
    /// Both must have come from [`Xhci::interrupt_ack`] for a controller that
    /// is still mapped.
    pub const unsafe fn from_addresses(usbsts: usize, iman: usize) -> Self {
        Self { usbsts, iman }
    }

    #[inline]
    pub const fn usbsts_address(&self) -> usize {
        self.usbsts
    }

    #[inline]
    pub const fn iman_address(&self) -> usize {
        self.iman
    }

    /// Acknowledges and masks the interrupter.  Returns false if this
    /// controller was not the one asserting, so a shared line can be passed on.
    ///
    /// # Safety
    /// The controller's registers must still be mapped.
    pub unsafe fn acknowledge(&self) -> bool {
        unsafe {
            let iman = (self.iman as *const u32).read_volatile();
            if iman & regs::iman::INTERRUPT_PENDING == 0 {
                return false;
            }
            // `IP` is write-1-to-clear and `IE` is written as zero, so this
            // both acknowledges and masks in one store.
            (self.iman as *mut u32).write_volatile(regs::iman::INTERRUPT_PENDING);
            let status = (self.usbsts as *const u32).read_volatile();
            if status & regs::usbsts::EVENT_INTERRUPT != 0 {
                (self.usbsts as *mut u32).write_volatile(regs::usbsts::EVENT_INTERRUPT);
            }
            true
        }
    }
}

/// The registers that describe where the controller thinks its rings are.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RegisterSnapshot {
    pub usbcmd: u32,
    pub usbsts: u32,
    pub crcr: u64,
    pub dcbaap: u64,
    pub config: u32,
    pub erstsz: u32,
    pub erstba: u64,
    pub erdp: u64,
    pub iman: u32,
    /// What the driver programmed, for comparison with the registers above.
    pub command_ring: u64,
    pub event_ring: u64,
    pub erst: u64,
    pub dcbaa: u64,
}

/// A small fixed-size queue of events waiting to be claimed.
///
/// One slot is not enough: a single [`Xhci::poll_events`] call can drain both
/// the Data Stage and the Status Stage event of a control transfer, and the
/// residual byte count only appears on the first of them.
struct EventQueue<const N: usize> {
    events: [Trb; N],
    head: usize,
    len: usize,
    /// Events dropped because the queue was full.  A non-zero count means a
    /// waiter is about to time out, so it is worth reporting.
    dropped: u64,
}

impl<const N: usize> EventQueue<N> {
    const fn new() -> Self {
        Self {
            events: [Trb {
                parameter: 0,
                status: 0,
                control: 0,
            }; N],
            head: 0,
            len: 0,
            dropped: 0,
        }
    }

    fn push(&mut self, event: Trb) {
        if self.len == N {
            self.dropped += 1;
            return;
        }
        self.events[(self.head + self.len) % N] = event;
        self.len += 1;
    }

    fn pop(&mut self) -> Option<Trb> {
        if self.len == 0 {
            return None;
        }
        let event = self.events[self.head];
        self.head = (self.head + 1) % N;
        self.len -= 1;
        Some(event)
    }

    fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
}

/// A command or transfer ring: the TRBs plus the producer's position in them.
struct Ring<'e, E: XhciEnv> {
    trbs: Dma<'e, E, Trb>,
    state: RingState,
}

impl<'e, E: XhciEnv> Ring<'e, E> {
    fn new(env: &'e E) -> Option<Self> {
        let trbs: Dma<'e, E, Trb> = Dma::new(env, RING_TRBS, 64)?;
        let state = RingState::new(RING_TRBS);
        // Publish the Link TRB up front, carrying the first lap's cycle bit.
        // The controller reaches it only after consuming everything ahead of
        // it, and by then it is the entry that must already be there — a Link
        // written after the TRB that leads to it can be missed by a controller
        // that has already stopped at an unowned entry.
        trbs.write(RING_TRBS - 1, state.link_trb(trbs.device_address()));
        env.write_barrier();
        Some(Self { trbs, state })
    }

    #[inline]
    fn base(&self) -> u64 {
        self.trbs.device_address()
    }

    /// The address the TRB about to be enqueued will live at, which is what a
    /// Command Completion or Transfer Event points back to.
    #[inline]
    fn next_address(&self) -> u64 {
        self.trbs.element_address(self.state.enqueue_index())
    }

    /// Writes one TRB, publishing its control word — and with it the cycle
    /// bit that hands the TRB over — after the rest of it.
    ///
    /// The fields are written separately rather than as one 16-byte store:
    /// the controller may already be walking this ring, and a store the
    /// compiler splits could otherwise leave it running a TRB whose parameter
    /// has not landed yet.
    fn push(&mut self, env: &E, value: Trb) {
        let index = self.state.enqueue_index();
        let control = (value.control & !trb_flags::CYCLE)
            | if self.state.cycle() {
                trb_flags::CYCLE
            } else {
                0
            };
        unsafe {
            let trb = self.trbs.as_ptr().add(index);
            (&raw mut (*trb).parameter).write_volatile(value.parameter);
            (&raw mut (*trb).status).write_volatile(value.status);
            env.write_barrier();
            (&raw mut (*trb).control).write_volatile(control);
        }
        env.write_barrier();

        if let Some(wrap) = self.state.advance() {
            // The Link TRB has to carry the cycle of the lap that just
            // finished, which is the one `advance` moved away from.
            let link = self.state.link_trb(self.base());
            let control = (link.control & !trb_flags::CYCLE)
                | if wrap.link_cycle { trb_flags::CYCLE } else { 0 };
            unsafe {
                let trb = self.trbs.as_ptr().add(wrap.link_index);
                (&raw mut (*trb).parameter).write_volatile(link.parameter);
                (&raw mut (*trb).status).write_volatile(link.status);
                env.write_barrier();
                (&raw mut (*trb).control).write_volatile(control);
            }
            env.write_barrier();
        }
    }
}

/// One periodic IN endpoint of a device, with its own ring and buffer.
struct InterruptEndpoint<'e, E: XhciEnv> {
    dci: u8,
    ring: Ring<'e, E>,
    buffer: Dma<'e, E, u8>,
    max_packet_size: u16,
    /// Address of the TRB of the transfer in flight, if there is one.
    in_flight: Option<u64>,
    /// The completion for that transfer, once it has arrived.
    completed: Option<Trb>,
}

/// A device that has been given a slot.
struct DeviceSlot<'e, E: XhciEnv> {
    device_context: Dma<'e, E, u32>,
    input_context: Dma<'e, E, u32>,
    control_ring: Ring<'e, E>,
    /// Interrupt IN endpoints, in the order they were configured.  A device
    /// can have several — a keyboard with a Boot Protocol interface and a
    /// second one for media keys, a receiver that is a keyboard and a mouse at
    /// once — so each is addressed by its device context index.
    interrupt_in: [Option<InterruptEndpoint<'e, E>>; MAX_INTERRUPT_ENDPOINTS],
    root_port: u8,
    speed: UsbSpeed,
    max_packet_size: u16,
}

pub struct Xhci<'e, E: XhciEnv> {
    env: &'e E,
    regs: Regs,
    capabilities: Capabilities,
    layout: ContextLayout,
    slots_enabled: u8,

    dcbaa: Dma<'e, E, u64>,
    scratchpad_array: Option<Dma<'e, E, u64>>,
    scratchpad_buffers: Vec<Dma<'e, E, u8>>,
    command_ring: Ring<'e, E>,
    event_ring: Dma<'e, E, Trb>,
    event_state: EventRingState,
    erst: Dma<'e, E, ErstEntry>,
    bounce: Dma<'e, E, u8>,

    /// Slot 0 is not used by the hardware, so index 0 stays empty.
    slots: Box<[Option<DeviceSlot<'e, E>>]>,

    /// Whether completions are expected to arrive as interrupts.
    interrupts: bool,

    /// Command Completion events waiting to be claimed.
    command_events: EventQueue<4>,
    /// Transfer Events waiting to be claimed.
    transfer_events: EventQueue<16>,
    /// Ports whose status changed since the caller last asked.
    port_change: u64,

    snapshot: XhciSnapshot,
}

impl<'e, E: XhciEnv> Xhci<'e, E> {
    /// Resets and starts the controller at `mmio_base`.
    ///
    /// # Safety
    /// `mmio_base` must be the mapped BAR of an xHCI controller whose bus
    /// mastering is already enabled, and must stay mapped for the lifetime of
    /// the returned value.
    pub unsafe fn new(mmio_base: usize, env: &'e E) -> Result<Self, UsbError> {
        let regs = unsafe { Regs::new(mmio_base) };
        let capabilities = unsafe { Capabilities::read(&regs) };
        if capabilities.max_ports == 0 || capabilities.max_slots == 0 {
            return Err(UsbError::Unsupported);
        }
        if capabilities.page_size & 0x1000 == 0 {
            // The allocator hands out 4 KiB pages; a controller that cannot
            // use them would need a different DMA contract.
            return Err(UsbError::Unsupported);
        }

        let layout = ContextLayout::new(capabilities.context_bytes());
        let slots_enabled = capabilities.max_slots.min(MAX_SLOTS);

        unsafe { Self::halt_and_reset(&regs, env)? };

        let dcbaa = Dma::new(env, slots_enabled as usize + 1, 64).ok_or(UsbError::Dma)?;
        let command_ring = Ring::new(env).ok_or(UsbError::Dma)?;
        let event_ring: Dma<'e, E, Trb> = Dma::new(env, RING_TRBS, 64).ok_or(UsbError::Dma)?;
        let erst: Dma<'e, E, ErstEntry> = Dma::new(env, 1, 64).ok_or(UsbError::Dma)?;
        let bounce = Dma::new(env, BOUNCE_BYTES, 64).ok_or(UsbError::Dma)?;

        let mut slots = Vec::new();
        slots.resize_with(slots_enabled as usize + 1, || None);

        let mut xhci = Self {
            env,
            regs,
            capabilities,
            layout,
            slots_enabled,
            dcbaa,
            scratchpad_array: None,
            scratchpad_buffers: Vec::new(),
            command_ring,
            event_ring,
            event_state: EventRingState::new(RING_TRBS),
            erst,
            bounce,
            slots: slots.into_boxed_slice(),
            interrupts: false,
            command_events: EventQueue::new(),
            transfer_events: EventQueue::new(),
            port_change: 0,
            snapshot: XhciSnapshot::default(),
        };
        xhci.allocate_scratchpad()?;
        unsafe { xhci.start()? };
        Ok(xhci)
    }

    /// Stops the controller and resets it, leaving every register at its
    /// default.  A controller left running by firmware is stopped first: a
    /// reset while it is running is undefined.
    unsafe fn halt_and_reset(regs: &Regs, env: &E) -> Result<(), UsbError> {
        unsafe {
            Self::wait_until(env, RESET_TIMEOUT_US, || {
                regs.read_op_u32(regs::op::USBSTS) & regs::usbsts::CONTROLLER_NOT_READY == 0
            })?;

            let command = regs.read_op_u32(regs::op::USBCMD);
            if command & regs::usbcmd::RUN != 0 {
                regs.write_op_u32(regs::op::USBCMD, command & !regs::usbcmd::RUN);
                Self::wait_until(env, RESET_TIMEOUT_US, || {
                    regs.read_op_u32(regs::op::USBSTS) & regs::usbsts::HALTED != 0
                })?;
            }

            regs.write_op_u32(regs::op::USBCMD, regs::usbcmd::HOST_CONTROLLER_RESET);
            // Both bits have to settle: `HCRST` clears when the reset is done,
            // `CNR` when the controller will answer register writes again.
            Self::wait_until(env, RESET_TIMEOUT_US, || {
                regs.read_op_u32(regs::op::USBCMD) & regs::usbcmd::HOST_CONTROLLER_RESET == 0
            })?;
            Self::wait_until(env, RESET_TIMEOUT_US, || {
                regs.read_op_u32(regs::op::USBSTS) & regs::usbsts::CONTROLLER_NOT_READY == 0
            })?;
        }
        Ok(())
    }

    /// Scratchpad buffers are RAM the controller keeps for itself.  A
    /// controller that asks for them will not start without them.
    fn allocate_scratchpad(&mut self) -> Result<(), UsbError> {
        let count = self.capabilities.max_scratchpad_buffers as usize;
        if count == 0 {
            return Ok(());
        }
        let array: Dma<'e, E, u64> = Dma::new(self.env, count, 64).ok_or(UsbError::Dma)?;
        let page = self.capabilities.page_size as usize;
        for index in 0..count {
            let buffer: Dma<'e, E, u8> = Dma::new(self.env, page, 4096).ok_or(UsbError::Dma)?;
            array.write(index, buffer.device_address());
            self.scratchpad_buffers.push(buffer);
        }
        // DCBAA entry 0 is the scratchpad array, not a device context.
        self.dcbaa.write(0, array.device_address());
        self.scratchpad_array = Some(array);
        Ok(())
    }

    unsafe fn start(&mut self) -> Result<(), UsbError> {
        unsafe {
            self.regs
                .write_op_u32(regs::op::CONFIG, self.slots_enabled as u32);
            self.regs
                .write_op_u64(regs::op::DCBAAP, self.dcbaa.device_address());

            // The command ring starts with `RCS` set, matching the producer's
            // initial cycle bit.
            self.regs.write_op_u64(
                regs::op::CRCR,
                self.command_ring.base() | regs::crcr::RING_CYCLE_STATE,
            );

            self.erst.write(
                0,
                ErstEntry {
                    ring_segment_base: self.event_ring.device_address(),
                    ring_segment_size: RING_TRBS as u32,
                    reserved: 0,
                },
            );
            self.env.write_barrier();
            // ERSTSZ before ERSTBA: writing the base is what makes the
            // controller latch the table, so the size has to be there already.
            self.regs.write_interrupter_u32(0, regs::runtime::ERSTSZ, 1);
            self.regs.write_interrupter_u64(
                0,
                regs::runtime::ERDP,
                self.event_ring.device_address(),
            );
            self.regs
                .write_interrupter_u64(0, regs::runtime::ERSTBA, self.erst.device_address());

            // Interrupts stay masked: completions are polled for now, and an
            // unhandled level interrupt from a controller nobody serviced
            // would livelock the boot.
            self.regs
                .write_interrupter_u32(0, regs::runtime::IMAN, regs::iman::INTERRUPT_PENDING);

            self.env.write_barrier();
            self.regs.write_op_u32(
                regs::op::USBCMD,
                regs::usbcmd::RUN | regs::usbcmd::HOST_SYSTEM_ERROR_ENABLE,
            );
            Self::wait_until(self.env, RESET_TIMEOUT_US, || {
                self.regs.read_op_u32(regs::op::USBSTS) & regs::usbsts::HALTED == 0
            })?;
        }
        Ok(())
    }

    /// Spins until `condition` holds or `timeout_us` passes.
    fn wait_until(
        env: &E,
        timeout_us: u64,
        mut condition: impl FnMut() -> bool,
    ) -> Result<(), UsbError> {
        let deadline = env.now_us().wrapping_add(timeout_us);
        loop {
            env.read_barrier();
            if condition() {
                return Ok(());
            }
            if env.now_us().wrapping_sub(deadline) < i64::MAX as u64 {
                return Err(UsbError::Timeout);
            }
            core::hint::spin_loop();
        }
    }

    /// The token an interrupt handler needs to silence this controller.
    pub fn interrupt_ack(&self) -> InterruptAck {
        InterruptAck {
            usbsts: self.regs.op_address(regs::op::USBSTS),
            iman: self.regs.interrupter_address(0, regs::runtime::IMAN),
        }
    }

    /// Lets the controller raise interrupts for events.
    ///
    /// Only worth doing once something is draining the event ring: until then
    /// a level-triggered line would be re-taken with nobody to service it.
    /// The handler masks the interrupter on every entry and
    /// [`Self::poll_events`] re-arms it, so the two cannot outrun each other.
    pub fn enable_interrupts(&mut self) {
        unsafe {
            self.regs.write_interrupter_u32(
                0,
                regs::runtime::IMAN,
                regs::iman::INTERRUPT_PENDING | regs::iman::INTERRUPT_ENABLE,
            );
            let command = self.regs.read_op_u32(regs::op::USBCMD);
            self.regs
                .write_op_u32(regs::op::USBCMD, command | regs::usbcmd::INTERRUPTER_ENABLE);
        }
        self.interrupts = true;
    }

    /// Stops the controller raising interrupts, leaving polling as the only
    /// way completions are noticed.
    pub fn disable_interrupts(&mut self) {
        unsafe {
            let command = self.regs.read_op_u32(regs::op::USBCMD);
            self.regs.write_op_u32(
                regs::op::USBCMD,
                command & !regs::usbcmd::INTERRUPTER_ENABLE,
            );
            self.regs
                .write_interrupter_u32(0, regs::runtime::IMAN, regs::iman::INTERRUPT_PENDING);
        }
        self.interrupts = false;
    }

    #[inline]
    pub fn interrupts_enabled(&self) -> bool {
        self.interrupts
    }

    /// The platform services this controller was built with.
    pub fn env(&self) -> &'e E {
        self.env
    }

    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    pub fn snapshot(&self) -> XhciSnapshot {
        XhciSnapshot {
            dropped_events: self.command_events.dropped + self.transfer_events.dropped,
            ..self.snapshot
        }
    }

    /// A snapshot of the registers that say whether the controller is running
    /// and where its rings are.  The first thing worth looking at when no
    /// events arrive.
    pub fn register_snapshot(&self) -> RegisterSnapshot {
        unsafe {
            RegisterSnapshot {
                usbcmd: self.regs.read_op_u32(regs::op::USBCMD),
                usbsts: self.regs.read_op_u32(regs::op::USBSTS),
                crcr: self.regs.read_op_u64(regs::op::CRCR),
                dcbaap: self.regs.read_op_u64(regs::op::DCBAAP),
                config: self.regs.read_op_u32(regs::op::CONFIG),
                erstsz: self.regs.read_interrupter_u32(0, regs::runtime::ERSTSZ),
                erstba: self.regs.read_interrupter_u64(0, regs::runtime::ERSTBA),
                erdp: self.regs.read_interrupter_u64(0, regs::runtime::ERDP),
                iman: self.regs.read_interrupter_u32(0, regs::runtime::IMAN),
                command_ring: self.command_ring.base(),
                event_ring: self.event_ring.device_address(),
                erst: self.erst.device_address(),
                dcbaa: self.dcbaa.device_address(),
            }
        }
    }

    pub fn port_count(&self) -> u8 {
        self.capabilities.max_ports
    }

    /// The USB revision of a root hub port, from the Supported Protocol
    /// extended capabilities.
    ///
    /// A port that no capability claims is reported as `{0, 0}`; the caller
    /// skips it rather than guessing, because the register layout of a
    /// SuperSpeed port differs in ways that matter.
    pub fn port_protocol(&self, port: u8) -> PortProtocol {
        unsafe {
            regs::find_extended_capability(
                &self.regs,
                self.capabilities.extended_capabilities,
                |capability, header| {
                    if capability.id != regs::extended::SUPPORTED_PROTOCOL {
                        return None;
                    }
                    let ports = self.regs.read_cap_u32(capability.offset + 8);
                    let first = (ports & 0xff) as u8;
                    let count = ((ports >> 8) & 0xff) as u8;
                    if first == 0 || port < first || port as u16 >= first as u16 + count as u16 {
                        return None;
                    }
                    Some(PortProtocol {
                        major: (header >> 24) as u8,
                        minor: (header >> 16) as u8,
                    })
                },
            )
        }
        .unwrap_or_default()
    }

    pub fn port_status(&self, port: u8) -> PortStatus {
        let raw = unsafe { self.regs.read_portsc(port) };
        PortStatus {
            connected: raw & regs::portsc::CURRENT_CONNECT_STATUS != 0,
            enabled: raw & regs::portsc::PORT_ENABLED != 0,
            resetting: raw & regs::portsc::PORT_RESET != 0,
            powered: raw & regs::portsc::PORT_POWER != 0,
            speed: context::speed_from_port(regs::portsc::port_speed(raw)),
            protocol: self.port_protocol(port),
            raw,
        }
    }

    /// Turns port power on wherever the controller leaves it to software.
    pub fn power_ports(&mut self) {
        if !self.capabilities.port_power_control {
            return;
        }
        for port in 1..=self.capabilities.max_ports {
            let raw = unsafe { self.regs.read_portsc(port) };
            if raw & regs::portsc::PORT_POWER == 0 {
                unsafe {
                    self.regs.write_portsc(
                        port,
                        (raw & regs::portsc::PRESERVE_MASK) | regs::portsc::PORT_POWER,
                    )
                };
            }
        }
    }

    /// Clears every change bit of `port` without disturbing the rest.
    pub fn acknowledge_port_change(&mut self, port: u8) {
        let raw = unsafe { self.regs.read_portsc(port) };
        let changes = raw & regs::portsc::CHANGE_MASK;
        if changes != 0 {
            unsafe {
                self.regs
                    .write_portsc(port, (raw & regs::portsc::PRESERVE_MASK) | changes)
            };
        }
    }

    /// Starts a reset on `port`.  Use [`Self::poll_port_reset`] to finish it.
    ///
    /// On a USB 2.0 port the controller runs the whole reset and enables the
    /// port itself; there is no separate SET_ADDRESS-time enable as on DWC2.
    /// It is split in two because reset signalling takes tens of milliseconds
    /// and this runs from a service poll that must not block that long.
    pub fn begin_port_reset(&mut self, port: u8) -> Result<(), UsbError> {
        let raw = unsafe { self.regs.read_portsc(port) };
        if raw & regs::portsc::CURRENT_CONNECT_STATUS == 0 {
            return Err(UsbError::Disconnected);
        }
        unsafe {
            self.regs.write_portsc(
                port,
                (raw & regs::portsc::PRESERVE_MASK)
                    | regs::portsc::PORT_RESET
                    | regs::portsc::PORT_RESET_CHANGE,
            )
        };
        Ok(())
    }

    /// Reports the outcome of a reset started with [`Self::begin_port_reset`],
    /// or `None` while the port is still resetting.
    ///
    /// The caller supplies the deadline, so a port that never finishes costs
    /// whatever that caller decided to spend and nothing more.
    pub fn poll_port_reset(&mut self, port: u8) -> Option<Result<UsbSpeed, UsbError>> {
        let status = unsafe { self.regs.read_portsc(port) };
        if status & regs::portsc::CURRENT_CONNECT_STATUS == 0 {
            self.acknowledge_port_change(port);
            return Some(Err(UsbError::Disconnected));
        }
        if status & regs::portsc::PORT_RESET != 0 || status & regs::portsc::PORT_RESET_CHANGE == 0 {
            return None;
        }
        self.acknowledge_port_change(port);

        if status & regs::portsc::PORT_ENABLED == 0 {
            return Some(Err(UsbError::ControllerFault));
        }
        Some(
            context::speed_from_port(regs::portsc::port_speed(status)).ok_or(UsbError::Unsupported),
        )
    }

    /// Resets `port` and waits for it to enable, blocking up to 500 ms.
    ///
    /// Convenience for a caller that is not driving a service loop.
    pub fn reset_port(&mut self, port: u8) -> Result<UsbSpeed, UsbError> {
        self.begin_port_reset(port)?;
        let deadline = self.env.now_us().wrapping_add(500_000);
        loop {
            if let Some(result) = self.poll_port_reset(port) {
                return result;
            }
            if self.env.now_us().wrapping_sub(deadline) < i64::MAX as u64 {
                return Err(UsbError::Timeout);
            }
            core::hint::spin_loop();
        }
    }

    /// Drains the event ring, filing each event where the waiters look.
    ///
    /// Returns the number of events consumed.  Events arrive for things nobody
    /// is waiting on — a port change during a transfer, say — so this keeps
    /// going rather than stopping at the first one.
    pub fn poll_events(&mut self) -> usize {
        self.env.note_foreground_progress();
        let mut consumed = 0;
        loop {
            self.env.read_barrier();
            let index = self.event_state.dequeue_index();
            let event = self.event_ring.read(index);
            if !self.event_state.owns(&event) {
                break;
            }
            self.event_state.advance();
            consumed += 1;
            self.snapshot.events += 1;
            match event.trb_type() {
                trb_type::COMMAND_COMPLETION_EVENT => self.command_events.push(event),
                trb_type::TRANSFER_EVENT => self.dispatch_transfer_event(event),
                trb_type::PORT_STATUS_CHANGE_EVENT => {
                    self.snapshot.port_events += 1;
                    let port = event.port_id();
                    if port >= 1 && port <= 64 {
                        self.port_change |= 1 << (port - 1);
                    }
                }
                trb_type::HOST_CONTROLLER_EVENT => self.snapshot.host_errors += 1,
                _ => self.snapshot.unexpected_events += 1,
            }
        }
        if consumed > 0 {
            // Handing the dequeue pointer back also clears `EHB`, which is
            // what lets the controller raise the next interrupt.
            let address = self
                .event_ring
                .element_address(self.event_state.dequeue_index());
            unsafe {
                self.regs.write_interrupter_u64(
                    0,
                    regs::runtime::ERDP,
                    address | regs::erdp::EVENT_HANDLER_BUSY,
                )
            };
        }
        if self.interrupts {
            // The handler masked the interrupter on its way in.  Re-arming
            // here, after the ring has been drained, is what makes the next
            // event raise a line again.  Unconditional on purpose: a spurious
            // entry that found nothing must not leave it masked forever.
            unsafe {
                self.regs.write_interrupter_u32(
                    0,
                    regs::runtime::IMAN,
                    regs::iman::INTERRUPT_PENDING | regs::iman::INTERRUPT_ENABLE,
                )
            };
        }
        consumed
    }

    /// Files a Transfer Event where whoever is waiting for it will look.
    ///
    /// The event carries the slot and the endpoint it belongs to, so an
    /// interrupt completion arriving while a control transfer is in flight
    /// goes to that endpoint's mailbox instead of being discarded by the
    /// control waiter.
    fn dispatch_transfer_event(&mut self, event: Trb) {
        let slot = event.slot_id() as usize;
        if event.endpoint_id() != context::DCI_CONTROL
            && let Some(device) = self.slots.get_mut(slot).and_then(|s| s.as_mut())
            && let Some(endpoint) = device
                .interrupt_in
                .iter_mut()
                .flatten()
                .find(|e| e.dci == event.endpoint_id())
        {
            if endpoint.completed.is_some() {
                // The previous completion was never claimed.  Keeping the
                // newer report is the right way round for a keyboard: the
                // stale one describes keys that have already changed.
                self.snapshot.dropped_reports += 1;
            }
            endpoint.completed = Some(event);
            return;
        }
        self.transfer_events.push(event);
    }

    /// Takes the set of root ports that reported a change, clearing it.
    pub fn take_port_changes(&mut self) -> u64 {
        core::mem::take(&mut self.port_change)
    }

    /// Acknowledges the write-1-to-clear bits of `USBSTS`, and reports a
    /// controller that has faulted.
    pub fn check_status(&mut self) -> Result<(), UsbError> {
        let status = unsafe { self.regs.read_op_u32(regs::op::USBSTS) };
        let acknowledge = status & regs::usbsts::RW1C_MASK;
        if acknowledge != 0 {
            unsafe { self.regs.write_op_u32(regs::op::USBSTS, acknowledge) };
        }
        if status & regs::usbsts::HOST_CONTROLLER_ERROR != 0 {
            self.snapshot.host_errors += 1;
            self.snapshot.last_fault_status = status;
            return Err(UsbError::ControllerFault);
        }
        if status & regs::usbsts::HOST_SYSTEM_ERROR != 0 {
            self.snapshot.host_errors += 1;
            self.snapshot.last_fault_status = status;
            return Err(UsbError::Dma);
        }
        Ok(())
    }

    /// Runs one command to completion.
    fn run_command(&mut self, command: Trb) -> Result<Trb, UsbError> {
        self.command_events.clear();
        let address = self.command_ring.next_address();
        self.command_ring.push(self.env, command);
        unsafe { self.regs.ring_doorbell(0, 0) };
        self.snapshot.commands += 1;

        let deadline = self.env.now_us().wrapping_add(COMMAND_TIMEOUT_US);
        loop {
            self.poll_events();
            while let Some(event) = self.command_events.pop() {
                // A completion for an older command would make the caller act
                // on the wrong result, so only the expected one is accepted.
                if event.parameter & !0xf == address & !0xf {
                    return match trb::completion_to_error(event.completion_code()) {
                        Ok(()) => Ok(event),
                        Err(error) => {
                            self.snapshot.command_errors += 1;
                            self.snapshot.last_failed_command = command.trb_type();
                            self.snapshot.last_failed_completion = event.completion_code();
                            Err(error)
                        }
                    };
                }
                self.snapshot.unexpected_events += 1;
            }
            self.check_status()?;
            if self.env.now_us().wrapping_sub(deadline) < i64::MAX as u64 {
                self.snapshot.command_timeouts += 1;
                return Err(UsbError::Timeout);
            }
            core::hint::spin_loop();
        }
    }

    /// Runs a No-Op command and waits for its completion.
    ///
    /// The cheapest end-to-end check there is: it exercises the command ring,
    /// the doorbell, the event ring and the cycle bits without touching a
    /// device, so a failure here points at the controller's own plumbing
    /// rather than at anything on the bus.
    pub fn no_op(&mut self) -> Result<(), UsbError> {
        self.run_command(Trb {
            parameter: 0,
            status: 0,
            control: trb_flags::trb_type(trb_type::NO_OP_COMMAND),
        })
        .map(|_| ())
    }

    /// Asks the controller for a Slot ID.
    pub fn enable_slot(&mut self) -> Result<u8, UsbError> {
        let event = self.run_command(Trb {
            parameter: 0,
            status: 0,
            control: trb_flags::trb_type(trb_type::ENABLE_SLOT),
        })?;
        let slot = event.slot_id();
        if slot == 0 || slot as usize >= self.slots.len() {
            return Err(UsbError::ResourceExhausted);
        }
        Ok(slot)
    }

    /// Releases a slot and everything allocated for it.
    ///
    /// The contexts and rings are only freed once the controller has
    /// acknowledged the command.  If it does not — a timeout, or a fault —
    /// they are deliberately left allocated: the controller may still be
    /// walking them, and handing that memory back to the allocator would let
    /// something else be overwritten by a DMA nobody is expecting.
    pub fn disable_slot(&mut self, slot: u8) -> Result<(), UsbError> {
        let result = self.run_command(Trb {
            parameter: 0,
            status: 0,
            control: trb_flags::trb_type(trb_type::DISABLE_SLOT) | ((slot as u32) << 24),
        });
        if result.is_err() {
            self.snapshot.leaked_slots += 1;
            return result.map(|_| ());
        }
        if (slot as usize) < self.dcbaa.len() {
            self.dcbaa.write(slot as usize, 0);
            self.env.write_barrier();
        }
        if let Some(entry) = self.slots.get_mut(slot as usize) {
            *entry = None;
        }
        Ok(())
    }

    /// Sets up a slot's contexts and gives the device an address.
    pub fn address_device(
        &mut self,
        slot: u8,
        route: DeviceRoute,
        speed: UsbSpeed,
    ) -> Result<(), UsbError> {
        let index = slot as usize;
        if index == 0 || index >= self.slots.len() {
            return Err(UsbError::InvalidRequest);
        }

        let device_context: Dma<'e, E, u32> =
            Dma::new(self.env, self.layout.device_context_bytes() / 4, 64).ok_or(UsbError::Dma)?;
        let input_context: Dma<'e, E, u32> =
            Dma::new(self.env, self.layout.input_context_bytes() / 4, 64).ok_or(UsbError::Dma)?;
        let control_ring = Ring::new(self.env).ok_or(UsbError::Dma)?;
        let max_packet_size = context::default_max_packet_size(speed);

        // Input Control Context: add the Slot Context and EP0.
        input_context.write(self.layout.input_control_word(0), 0);
        input_context.write(self.layout.input_control_word(1), 0b11);

        let slot_words = SlotContextFields {
            route_string: route.route_string,
            speed: context::slot_speed(speed),
            context_entries: context::DCI_CONTROL,
            root_hub_port: route.root_port,
            tt_hub_slot: route.tt_hub_slot,
            tt_port: route.tt_port,
            tt_think_time: route.tt_think_time,
            ..Default::default()
        }
        .words();
        for (word, value) in slot_words.iter().enumerate() {
            input_context.write(self.layout.input_slot_word(word), *value);
        }

        let ep0_words =
            EndpointContextFields::control(max_packet_size, control_ring.base()).words();
        for (word, value) in ep0_words.iter().enumerate() {
            input_context.write(
                self.layout.input_endpoint_word(context::DCI_CONTROL, word),
                *value,
            );
        }

        self.dcbaa.write(index, device_context.device_address());
        self.env.write_barrier();

        let input_address = input_context.device_address();
        self.slots[index] = Some(DeviceSlot {
            device_context,
            input_context,
            control_ring,
            interrupt_in: [const { None }; MAX_INTERRUPT_ENDPOINTS],
            root_port: route.root_port,
            speed,
            max_packet_size,
        });

        let result = self.run_command(Trb {
            parameter: input_address,
            status: 0,
            control: trb_flags::trb_type(trb_type::ADDRESS_DEVICE) | ((slot as u32) << 24),
        });
        if result.is_err() {
            self.dcbaa.write(index, 0);
            self.env.write_barrier();
            self.slots[index] = None;
        }
        result.map(|_| ())
    }

    /// Tells the controller this slot is a hub, so it can route to what is
    /// behind it.
    ///
    /// Until this is set the controller treats the slot as a leaf and an
    /// Address Device carrying a Route String through it is rejected.
    /// `think_time` comes from the hub descriptor and only matters for a High
    /// Speed hub, which is where the transaction translator lives.
    pub fn configure_hub(&mut self, slot: u8, ports: u8, think_time: u8) -> Result<(), UsbError> {
        let index = slot as usize;
        let Some(device) = self.slots.get(index).and_then(|s| s.as_ref()) else {
            return Err(UsbError::InvalidRequest);
        };
        let input = &device.input_context;

        // Evaluate Context against the Slot Context alone: everything else it
        // holds is copied from the context the controller owns, so the
        // address it assigned survives unchanged.
        for word in 0..4 {
            let value = device
                .device_context
                .read(self.layout.device_slot_word(word));
            input.write(self.layout.input_slot_word(word), value);
        }
        let dw0 = input.read(self.layout.input_slot_word(0));
        // `Hub` is bit 26; `MTT` (bit 25) stays clear, which puts a multi-TT
        // hub in its single-TT mode — always valid, and it needs no
        // SET_INTERFACE to select.
        input.write(self.layout.input_slot_word(0), dw0 | (1 << 26));
        let dw1 = input.read(self.layout.input_slot_word(1));
        input.write(
            self.layout.input_slot_word(1),
            (dw1 & 0x00ff_ffff) | ((ports as u32) << 24),
        );
        let dw2 = input.read(self.layout.input_slot_word(2));
        input.write(
            self.layout.input_slot_word(2),
            (dw2 & !(3 << 16)) | ((think_time as u32 & 3) << 16),
        );
        input.write(self.layout.input_control_word(0), 0);
        input.write(self.layout.input_control_word(1), 1);
        let input_address = input.device_address();
        self.env.write_barrier();

        // Evaluate Context only looks at a Slot Context's Interrupter Target
        // and Max Exit Latency; the hub fields are evaluated by Configure
        // Endpoint.  Linux (`xhci_update_hub_device`) uses Configure Endpoint
        // on anything newer than 0.95 and Evaluate Context only on 0.95
        // itself.  QEMU accepts either, which is why this went unnoticed until
        // a VL805 refused every Address Device behind the hub.
        let command = if self.capabilities.hci_version > 0x0095 {
            trb_type::CONFIGURE_ENDPOINT
        } else {
            trb_type::EVALUATE_CONTEXT
        };
        self.run_command(Trb {
            parameter: input_address,
            status: 0,
            control: trb_flags::trb_type(command) | ((slot as u32) << 24),
        })
        .map(|_| ())
    }

    /// The USB address the controller assigned to `slot`, read back from the
    /// Slot Context it owns.
    pub fn device_address(&self, slot: u8) -> Option<u8> {
        let device = self.slots.get(slot as usize)?.as_ref()?;
        self.env.read_barrier();
        Some(device.device_context.read(self.layout.device_slot_word(3)) as u8)
    }

    pub fn slot_speed(&self, slot: u8) -> Option<UsbSpeed> {
        Some(self.slots.get(slot as usize)?.as_ref()?.speed)
    }

    pub fn slot_root_port(&self, slot: u8) -> Option<u8> {
        Some(self.slots.get(slot as usize)?.as_ref()?.root_port)
    }

    /// Tells the controller EP0's packet size changed, after the first eight
    /// bytes of the device descriptor have been read.
    ///
    /// Only a Full Speed device needs this: its EP0 may be 8, 16, 32 or 64
    /// bytes and cannot be known before the descriptor arrives.
    pub fn set_max_packet_size(&mut self, slot: u8, max_packet_size: u16) -> Result<(), UsbError> {
        let index = slot as usize;
        let Some(device) = self.slots.get_mut(index).and_then(|s| s.as_mut()) else {
            return Err(UsbError::InvalidRequest);
        };
        if device.max_packet_size == max_packet_size {
            return Ok(());
        }
        device.max_packet_size = max_packet_size;
        let input_address = device.input_context.device_address();

        // Evaluate Context looks only at EP0 here; the Slot Context is not
        // added, so the address the controller assigned is left alone.
        device
            .input_context
            .write(self.layout.input_control_word(0), 0);
        device
            .input_context
            .write(self.layout.input_control_word(1), 1 << context::DCI_CONTROL);
        let words =
            EndpointContextFields::control(max_packet_size, device.control_ring.base()).words();
        for (word, value) in words.iter().enumerate() {
            device.input_context.write(
                self.layout.input_endpoint_word(context::DCI_CONTROL, word),
                *value,
            );
        }
        self.env.write_barrier();

        self.run_command(Trb {
            parameter: input_address,
            status: 0,
            control: trb_flags::trb_type(trb_type::EVALUATE_CONTEXT) | ((slot as u32) << 24),
        })
        .map(|_| ())
    }

    /// The running interrupt IN endpoint `dci` of `slot`.
    fn interrupt_endpoint(&self, slot: u8, dci: u8) -> Option<&InterruptEndpoint<'e, E>> {
        self.slots
            .get(slot as usize)?
            .as_ref()?
            .interrupt_in
            .iter()
            .flatten()
            .find(|e| e.dci == dci)
    }

    fn interrupt_endpoint_mut(
        &mut self,
        slot: u8,
        dci: u8,
    ) -> Option<&mut InterruptEndpoint<'e, E>> {
        self.slots
            .get_mut(slot as usize)?
            .as_mut()?
            .interrupt_in
            .iter_mut()
            .flatten()
            .find(|e| e.dci == dci)
    }

    /// Adds an interrupt IN endpoint to `slot` and starts it, returning its
    /// device context index, which the other interrupt calls take.
    ///
    /// `b_interval` is the endpoint descriptor's field as it stands; the
    /// encoding difference between the speeds is handled here.  Endpoints
    /// already running on the slot are left as they are, so a device with
    /// several keyboard interfaces is configured one endpoint at a time.
    pub fn configure_interrupt_in(
        &mut self,
        slot: u8,
        endpoint: EndpointAddress,
        max_packet_size: u16,
        b_interval: u8,
    ) -> Result<u8, UsbError> {
        if endpoint.direction() != Direction::In || endpoint.number() == 0 {
            return Err(UsbError::InvalidRequest);
        }
        if max_packet_size == 0 || max_packet_size as usize > INTERRUPT_BUFFER_BYTES {
            return Err(UsbError::Buffer);
        }
        let index = slot as usize;
        let Some(device) = self.slots.get(index).and_then(|s| s.as_ref()) else {
            return Err(UsbError::InvalidRequest);
        };
        let speed = device.speed;
        let dci = context::device_context_index(endpoint.number(), Direction::In);
        if device.interrupt_in.iter().flatten().any(|e| e.dci == dci) {
            return Ok(dci);
        }
        let Some(free) = device.interrupt_in.iter().position(|e| e.is_none()) else {
            return Err(UsbError::ResourceExhausted);
        };

        let ring = Ring::new(self.env).ok_or(UsbError::Dma)?;
        let buffer: Dma<'e, E, u8> =
            Dma::new(self.env, INTERRUPT_BUFFER_BYTES, 64).ok_or(UsbError::Dma)?;

        let device = self.slots[index].as_mut().ok_or(UsbError::Disconnected)?;
        let input = &device.input_context;
        // Configure Endpoint takes the Slot Context along, because `Context
        // Entries` has to cover the highest endpoint in use.  Everything else
        // in it is copied from the context the controller owns, so the
        // address it assigned is carried over unchanged.
        for word in 0..4 {
            let value = device
                .device_context
                .read(self.layout.device_slot_word(word));
            input.write(self.layout.input_slot_word(word), value);
        }
        let dw0 = input.read(self.layout.input_slot_word(0));
        let entries = ((dw0 >> 27) as u8).max(dci);
        input.write(
            self.layout.input_slot_word(0),
            (dw0 & 0x07ff_ffff) | ((entries as u32) << 27),
        );
        // Add only the new endpoint; the others are neither added again nor
        // dropped, so they keep running across this command.
        input.write(self.layout.input_control_word(0), 0);
        input.write(self.layout.input_control_word(1), 1 | (1 << dci));

        let fields = EndpointContextFields {
            endpoint_type: context::endpoint_type(TransferType::Interrupt, Direction::In),
            max_packet_size,
            max_burst_size: 0,
            interval: context::interrupt_interval(speed, b_interval),
            error_count: 3,
            dequeue_pointer: ring.base(),
            dequeue_cycle: true,
            average_trb_length: max_packet_size,
            max_esit_payload: max_packet_size,
        };
        for (word, value) in fields.words().iter().enumerate() {
            input.write(self.layout.input_endpoint_word(dci, word), *value);
        }
        let input_address = input.device_address();
        self.env.write_barrier();

        device.interrupt_in[free] = Some(InterruptEndpoint {
            dci,
            ring,
            buffer,
            max_packet_size,
            in_flight: None,
            completed: None,
        });

        let result = self.run_command(Trb {
            parameter: input_address,
            status: 0,
            control: trb_flags::trb_type(trb_type::CONFIGURE_ENDPOINT) | ((slot as u32) << 24),
        });
        if result.is_err()
            && let Some(device) = self.slots.get_mut(index).and_then(|s| s.as_mut())
        {
            device.interrupt_in[free] = None;
        }
        result.map(|_| dci)
    }

    /// Brings a halted interrupt IN endpoint back into service.
    ///
    /// A Stall or a run of transaction errors leaves the endpoint Halted, and
    /// the controller will not touch its ring again until it is reset and told
    /// where to resume.  This does the host half — Reset Endpoint, then Set TR
    /// Dequeue Pointer at the ring's current enqueue position, which is where
    /// the next transfer will be written.  The caller still has to clear the
    /// halt on the device with CLEAR_FEATURE(ENDPOINT_HALT), and has to check
    /// the device is still there first: behind a hub, a slot outlives the
    /// device it was made for, so this succeeds for a device that has gone.
    pub fn recover_interrupt_in(&mut self, slot: u8, dci: u8) -> Result<(), UsbError> {
        let Some(endpoint) = self.interrupt_endpoint(slot, dci) else {
            return Err(UsbError::InvalidRequest);
        };
        let dequeue = endpoint.ring.next_address();
        let cycle = endpoint.ring.state.cycle();

        // Reset Endpoint moves a Halted endpoint to Stopped.  One that was
        // not halted answers with a context state error, which is not a
        // failure here — the ring still has to be re-pointed either way.
        match self.run_command(Trb {
            parameter: 0,
            status: 0,
            control: trb_flags::trb_type(trb_type::RESET_ENDPOINT)
                | ((dci as u32) << 16)
                | ((slot as u32) << 24),
        }) {
            Ok(_) | Err(UsbError::InvalidRequest) => {}
            Err(error) => return Err(error),
        }

        self.run_command(Trb {
            parameter: dequeue | cycle as u64,
            status: 0,
            control: trb_flags::trb_type(trb_type::SET_TR_DEQUEUE_POINTER)
                | ((dci as u32) << 16)
                | ((slot as u32) << 24),
        })?;

        if let Some(endpoint) = self.interrupt_endpoint_mut(slot, dci) {
            // Whatever was in flight is gone: the controller stopped at the
            // failed TRB and the dequeue pointer has been moved past it.
            endpoint.in_flight = None;
            endpoint.completed = None;
        }
        self.snapshot.endpoint_recoveries += 1;
        Ok(())
    }

    /// The endpoint address of interrupt IN endpoint `dci`, for a caller that
    /// has to clear the halt on the device itself.
    pub fn interrupt_in_endpoint(&self, slot: u8, dci: u8) -> Option<EndpointAddress> {
        self.interrupt_endpoint(slot, dci)?;
        // The device context index is `2 * number + 1` for an IN endpoint.
        EndpointAddress::new((dci - 1) / 2, Direction::In)
    }

    /// Queues one interrupt IN transfer on `dci`, if none is outstanding.
    ///
    /// Returns `Ok(false)` when a transfer is already in flight, which is the
    /// normal answer while waiting for a key.
    pub fn submit_interrupt_in(&mut self, slot: u8, dci: u8) -> Result<bool, UsbError> {
        if self
            .slots
            .get(slot as usize)
            .and_then(|s| s.as_ref())
            .is_none()
        {
            return Err(UsbError::Disconnected);
        }
        let env = self.env;
        let Some(endpoint) = self.interrupt_endpoint_mut(slot, dci) else {
            return Err(UsbError::InvalidRequest);
        };
        if endpoint.in_flight.is_some() {
            return Ok(false);
        }
        let address = endpoint.ring.next_address();
        let length = endpoint.max_packet_size as u32;
        let buffer = endpoint.buffer.device_address();
        endpoint.ring.push(
            env,
            Trb {
                parameter: buffer,
                status: length,
                control: trb_flags::trb_type(trb_type::NORMAL)
                    | trb_flags::INTERRUPT_ON_COMPLETION
                    | trb_flags::INTERRUPT_ON_SHORT_PACKET,
            },
        );
        endpoint.in_flight = Some(address);
        unsafe { self.regs.ring_doorbell(slot, dci) };
        self.snapshot.transfers += 1;
        Ok(true)
    }

    /// Collects a completed interrupt IN transfer on `dci`, if one finished.
    ///
    /// Copies at most `report.len()` bytes and returns how many the device
    /// sent.  Nothing is resubmitted here: the caller decides whether to keep
    /// polling, which is what lets a disconnect stop the cycle.
    pub fn poll_interrupt_in(
        &mut self,
        slot: u8,
        dci: u8,
        report: &mut [u8],
    ) -> Option<Result<usize, UsbError>> {
        self.poll_events();
        let env = self.env;
        let endpoint = self.interrupt_endpoint_mut(slot, dci)?;
        let event = endpoint.completed.take()?;
        endpoint.in_flight = None;

        match trb::completion_to_error(event.completion_code()) {
            Ok(()) => {
                let length = (endpoint.max_packet_size as usize)
                    .saturating_sub(event.transfer_length() as usize)
                    .min(report.len());
                env.read_barrier();
                for (offset, byte) in report[..length].iter_mut().enumerate() {
                    *byte = endpoint.buffer.read(offset);
                }
                Some(Ok(length))
            }
            Err(error) => {
                self.snapshot.transfer_errors += 1;
                Some(Err(error))
            }
        }
    }

    /// Runs one control transfer on `slot` and waits for it.
    ///
    /// `setup` is the eight-byte Setup packet.  `data` is the payload, at most
    /// [`BOUNCE_BYTES`]; it is copied through a DMA-capable bounce buffer, so
    /// the caller may pass an ordinary stack slice.  Returns how many bytes
    /// the device actually transferred.
    ///
    /// Events queued for anything else are dropped while this runs: see the
    /// module documentation on why nothing may overlap it yet.
    pub fn control_transfer(
        &mut self,
        slot: u8,
        setup: [u8; 8],
        data: &mut [u8],
        direction: Direction,
    ) -> Result<usize, UsbError> {
        if data.len() > BOUNCE_BYTES {
            return Err(UsbError::Buffer);
        }
        let index = slot as usize;
        if self.slots.get(index).and_then(|s| s.as_ref()).is_none() {
            return Err(UsbError::InvalidRequest);
        }

        if direction == Direction::Out && !data.is_empty() {
            for (offset, byte) in data.iter().enumerate() {
                self.bounce.write(offset, *byte);
            }
        } else {
            for offset in 0..data.len() {
                self.bounce.write(offset, 0);
            }
        }
        let buffer_address = self.bounce.device_address();
        let length = data.len() as u32;

        // Setup Stage: the eight bytes travel in the TRB itself.
        let setup_parameter = u64::from_le_bytes(setup);
        // Transfer Type: 0 = no data, 2 = OUT, 3 = IN.
        let transfer_type = if length == 0 {
            0u32
        } else if direction == Direction::In {
            3
        } else {
            2
        };
        // Status Stage runs the opposite way to the data.
        let status_in = length == 0 || direction == Direction::Out;
        // Only control events land here now, and the enumeration state
        // machine keeps one control transfer outstanding at a time, so
        // anything still queued is from a transfer that has already been
        // given up on.
        self.transfer_events.clear();
        let (data_address, status_address) = {
            let device = self.slots[index].as_mut().ok_or(UsbError::Disconnected)?;
            device.control_ring.push(
                self.env,
                Trb {
                    parameter: setup_parameter,
                    status: 8,
                    control: trb_flags::trb_type(trb_type::SETUP_STAGE)
                        | trb_flags::IMMEDIATE_DATA
                        | (transfer_type << 16),
                },
            );
            // The Data Stage carries its own interrupt, including on a short
            // packet: the residual count only appears on the event of the TRB
            // that fell short, and the Status Stage event would report a full
            // transfer.
            let data_address = (length > 0).then(|| {
                let address = device.control_ring.next_address();
                device.control_ring.push(
                    self.env,
                    Trb {
                        parameter: buffer_address,
                        status: length,
                        control: trb_flags::trb_type(trb_type::DATA_STAGE)
                            | trb_flags::INTERRUPT_ON_COMPLETION
                            | trb_flags::INTERRUPT_ON_SHORT_PACKET
                            | if direction == Direction::In {
                                trb_flags::DIRECTION_IN
                            } else {
                                0
                            },
                    },
                );
                address
            });
            let status_address = device.control_ring.next_address();
            device.control_ring.push(
                self.env,
                Trb {
                    parameter: 0,
                    status: 0,
                    control: trb_flags::trb_type(trb_type::STATUS_STAGE)
                        | trb_flags::INTERRUPT_ON_COMPLETION
                        | if status_in {
                            trb_flags::DIRECTION_IN
                        } else {
                            0
                        },
                },
            );
            (data_address, status_address)
        };
        unsafe { self.regs.ring_doorbell(slot, context::DCI_CONTROL) };
        self.snapshot.transfers += 1;

        let mut transferred = 0usize;
        let deadline = self.env.now_us().wrapping_add(COMMAND_TIMEOUT_US);
        'wait: loop {
            self.poll_events();
            while let Some(event) = self.transfer_events.pop() {
                let address = event.parameter & !0xf;
                let is_data = data_address.is_some_and(|a| a & !0xf == address);
                let is_status = status_address & !0xf == address;
                if !is_data && !is_status {
                    self.snapshot.unexpected_events += 1;
                    continue;
                }
                if let Err(error) = trb::completion_to_error(event.completion_code()) {
                    self.snapshot.transfer_errors += 1;
                    return Err(error);
                }
                if is_data {
                    // Only the Data Stage event carries the residual; the
                    // Status Stage always reports a length of zero.
                    let residual = event.transfer_length() as usize;
                    transferred = data.len().saturating_sub(residual);
                    continue;
                }
                break 'wait;
            }
            self.check_status()?;
            if self.env.now_us().wrapping_sub(deadline) < i64::MAX as u64 {
                self.snapshot.transfer_timeouts += 1;
                return Err(UsbError::Timeout);
            }
            core::hint::spin_loop();
        }

        if direction == Direction::In && transferred > 0 {
            self.env.read_barrier();
            for (offset, byte) in data[..transferred].iter_mut().enumerate() {
                *byte = self.bounce.read(offset);
            }
        }
        Ok(transferred)
    }
}

impl<E: XhciEnv> Drop for Xhci<'_, E> {
    fn drop(&mut self) {
        // Stop the controller before any of the DMA regions below are freed.
        unsafe {
            let command = self.regs.read_op_u32(regs::op::USBCMD);
            self.regs
                .write_op_u32(regs::op::USBCMD, command & !regs::usbcmd::RUN);
            let _ = Self::wait_until(self.env, RESET_TIMEOUT_US, || {
                self.regs.read_op_u32(regs::op::USBSTS) & regs::usbsts::HALTED != 0
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use super::*;

    /// An environment backed by the host allocator, for the parts of the
    /// driver that can be exercised without a controller.
    struct TestEnv {
        now: Cell<u64>,
        barriers: Cell<usize>,
    }

    impl TestEnv {
        fn new() -> Self {
            Self {
                now: Cell::new(0),
                barriers: Cell::new(0),
            }
        }
    }

    impl XhciEnv for TestEnv {
        fn now_us(&self) -> u64 {
            let now = self.now.get();
            self.now.set(now + 1);
            now
        }

        fn alloc_dma(&self, len: usize, align: usize) -> Option<DmaAllocation> {
            let layout = alloc::alloc::Layout::from_size_align(len, align).ok()?;
            let cpu = core::ptr::NonNull::new(unsafe { alloc::alloc::alloc_zeroed(layout) })?;
            Some(DmaAllocation {
                cpu,
                device: cpu.as_ptr() as u64,
                len,
                align,
            })
        }

        unsafe fn free_dma(&self, allocation: DmaAllocation) {
            if let Ok(layout) =
                alloc::alloc::Layout::from_size_align(allocation.len, allocation.align)
            {
                unsafe { alloc::alloc::dealloc(allocation.cpu.as_ptr(), layout) }
            }
        }

        fn write_barrier(&self) {
            self.barriers.set(self.barriers.get() + 1);
        }

        fn read_barrier(&self) {}
    }

    #[test]
    fn a_ring_publishes_the_link_trb_before_anything_can_reach_it() {
        let env = TestEnv::new();
        let ring = Ring::new(&env).expect("the test allocator has memory");
        let link = ring.trbs.read(RING_TRBS - 1);
        assert_eq!(link.trb_type(), trb_type::LINK);
        assert_eq!(link.parameter, ring.base());
        // The controller starts with cycle 1, so the Link has to be owned
        // from the outset or it would stop one TRB short of the wrap.
        assert!(link.cycle());
        assert!(link.control & trb_flags::TOGGLE_CYCLE != 0);
    }

    #[test]
    fn pushing_a_trb_sets_the_cycle_and_orders_the_control_word_last() {
        let env = TestEnv::new();
        let mut ring = Ring::new(&env).expect("the test allocator has memory");
        let before = env.barriers.get();
        ring.push(
            &env,
            Trb {
                parameter: 0xdead_beef,
                status: 8,
                control: trb_flags::trb_type(trb_type::NO_OP_COMMAND),
            },
        );
        let written = ring.trbs.read(0);
        assert_eq!(written.parameter, 0xdead_beef);
        assert_eq!(written.trb_type(), trb_type::NO_OP_COMMAND);
        assert!(written.cycle());
        // One barrier separates the body from the control word, one follows
        // it; without the first, a controller can run a half-written TRB.
        assert!(env.barriers.get() >= before + 2);
    }

    #[test]
    fn the_link_trb_is_rewritten_with_the_cycle_of_the_lap_that_just_ended() {
        let env = TestEnv::new();
        let mut ring = Ring::new(&env).expect("the test allocator has memory");
        let noop = Trb {
            parameter: 0,
            status: 0,
            control: trb_flags::trb_type(trb_type::NO_OP_COMMAND),
        };
        // Fill the segment: the last usable slot is RING_TRBS - 2.
        for _ in 0..RING_TRBS - 1 {
            ring.push(&env, noop);
        }
        let link = ring.trbs.read(RING_TRBS - 1);
        assert!(link.cycle(), "the first lap's Link must stay owned");
        assert_eq!(ring.state.enqueue_index(), 0);
        assert!(!ring.state.cycle(), "the producer flips after the wrap");

        // The second lap writes TRBs with the opposite cycle, and closes with
        // a Link carrying that one.
        for _ in 0..RING_TRBS - 1 {
            ring.push(&env, noop);
        }
        assert!(!ring.trbs.read(RING_TRBS - 1).cycle());
        assert!(ring.state.cycle());
    }

    #[test]
    fn the_event_queue_keeps_both_events_of_a_control_transfer() {
        // A single drain of the event ring can yield the Data Stage and the
        // Status Stage event together.  Only the first carries the residual,
        // so a one-slot latch would report every short read as a full one.
        let mut queue: EventQueue<4> = EventQueue::new();
        let data = Trb {
            parameter: 0x1000,
            status: 5,
            control: 0,
        };
        let status = Trb {
            parameter: 0x1010,
            status: 0,
            control: 0,
        };
        queue.push(data);
        queue.push(status);
        assert_eq!(queue.pop(), Some(data));
        assert_eq!(queue.pop(), Some(status));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn a_full_event_queue_counts_what_it_drops_instead_of_overwriting() {
        let mut queue: EventQueue<2> = EventQueue::new();
        let event = |n: u64| Trb {
            parameter: n,
            status: 0,
            control: 0,
        };
        queue.push(event(1));
        queue.push(event(2));
        queue.push(event(3));
        assert_eq!(queue.dropped, 1);
        assert_eq!(queue.pop(), Some(event(1)));
        assert_eq!(queue.pop(), Some(event(2)));
    }

    #[test]
    fn the_event_queue_wraps_without_losing_order() {
        let mut queue: EventQueue<2> = EventQueue::new();
        let event = |n: u64| Trb {
            parameter: n,
            status: 0,
            control: 0,
        };
        queue.push(event(1));
        assert_eq!(queue.pop(), Some(event(1)));
        queue.push(event(2));
        queue.push(event(3));
        assert_eq!(queue.pop(), Some(event(2)));
        assert_eq!(queue.pop(), Some(event(3)));
        assert_eq!(queue.dropped, 0);
    }
}

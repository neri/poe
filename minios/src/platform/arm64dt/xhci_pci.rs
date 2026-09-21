//! Attaching the common xHCI driver to a PCI controller.
//!
//! Stage 2 of `docs/USB_HOST_RPI4_PLAN.md`: on the QEMU virt machine this
//! finds `qemu-xhci` behind the generic ECAM host bridge, gives it a BAR and
//! bus mastering, and brings the controller up.  The same glue is what the
//! Raspberry Pi 4 will use once the BCM2711 host bridge is implemented, which
//! is why nothing here knows about QEMU in particular.
//!
//! The probe is bounded and advisory: every failure is reported and the boot
//! continues.  USB is not yet the console on this machine, and a controller
//! that will not start must not cost the UART.

use alloc::alloc::{alloc_zeroed, dealloc};
use core::alloc::Layout;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use super::pci::{self, Bdf, DmaWindow, PciHost};
use super::{counter_us, irq};
use crate::io::pci::ConfigAccess;
use crate::io::usb::input::UsbTextInputMux;
use crate::io::usb::xhci::device::XhciUsb;
use crate::io::usb::xhci::env::{DmaAllocation, XhciEnv};
use crate::io::usb::xhci::{InterruptAck, Xhci};
use crate::{System, usb_println};

/// The platform side of the xHCI driver's contract on an Arm device-tree
/// machine.
///
/// The MMU and the data cache are off in this kernel, so RAM is already
/// coherent with device DMA and the barriers only have to order accesses.
/// A port that turns the MMU on has to add cache maintenance here, which is
/// exactly why the driver asks for the barriers rather than issuing them.
pub struct DtXhciEnv {
    /// Where the host bridge lets the controller DMA, from its `dma-ranges`.
    /// A Raspberry Pi 4 with current firmware shows the controller RAM at a
    /// 16 GiB offset, so this is not an identity map in general.
    dma: DmaWindow,
}

impl DtXhciEnv {
    pub const fn new(dma: DmaWindow) -> Self {
        Self { dma }
    }
}

impl XhciEnv for DtXhciEnv {
    fn now_us(&self) -> u64 {
        counter_us()
    }

    fn alloc_dma(&self, len: usize, align: usize) -> Option<DmaAllocation> {
        let layout = Layout::from_size_align(len, align).ok()?;
        // The global allocator hands out page-aligned, page-granular, zeroed
        // memory, which is what the rings and contexts need: 64-byte aligned
        // and never straddling a 64 KiB boundary.
        let cpu = NonNull::new(unsafe { alloc_zeroed(layout) })?;
        let start = cpu.as_ptr() as u64;
        // Memory the controller cannot reach is worse than none: it would
        // accept the address and then read or write somewhere else entirely.
        if start < self.dma.cpu_start || start.saturating_add(len as u64) > self.dma.cpu_end {
            unsafe { dealloc(cpu.as_ptr(), layout) };
            return None;
        }
        Some(DmaAllocation {
            cpu,
            device: start.wrapping_add(self.dma.offset),
            len,
            align,
        })
    }

    unsafe fn free_dma(&self, allocation: DmaAllocation) {
        if let Ok(layout) = Layout::from_size_align(allocation.len, allocation.align) {
            unsafe { dealloc(allocation.cpu.as_ptr(), layout) }
        }
    }

    fn write_barrier(&self) {
        // `dsb sy` rather than a release fence: the following store may be to
        // device memory, which the compiler's atomics do not order against.
        unsafe { core::arch::asm!("dsb sy", options(nostack, preserves_flags)) };
    }

    fn read_barrier(&self) {
        unsafe { core::arch::asm!("dsb sy", options(nostack, preserves_flags)) };
    }

    fn note_foreground_progress(&self) {
        note_progress()
    }

    fn interrupt_stalled(&self) -> bool {
        interrupt_stormed()
    }
}

/// What the PCI walk found.
pub struct XhciPciDevice {
    pub bdf: Bdf,
    pub vendor: u16,
    pub device: u16,
    pub mmio_base: usize,
    pub mmio_size: u64,
    pub irq: Option<crate::arch::gic::Irq>,
    pub dma: DmaWindow,
}

/// Finds an xHCI controller on the device tree's ECAM host bridge and makes
/// it addressable.  Returns `Err` with a reason suitable for a boot message.
pub fn probe(tree: &fdt::DeviceTree) -> Result<XhciPciDevice, &'static str> {
    let mut host = PciHost::probe_ecam(tree).ok_or("no ECAM PCI host bridge in the device tree")?;
    probe_on(&mut host)
}

/// Finds an xHCI controller below `host`, whatever kind of host it is, and
/// makes it addressable: bridges numbered, BAR0 assigned, memory decoding and
/// bus mastering on.
pub fn probe_on<C: ConfigAccess>(host: &mut PciHost<C>) -> Result<XhciPciDevice, &'static str> {
    let bridges = host.configure_bridges();
    if bridges > 0 {
        usb_println!("PCI: {} bridge(s) configured", bridges);
    }
    let bdf = host
        .find_map(|host, bdf| {
            let (class, subclass, prog_if) = host.class_of(bdf);
            (class == pci::CLASS_SERIAL_USB
                && subclass == pci::SUBCLASS_USB
                && prog_if == pci::PROG_IF_XHCI)
                .then_some(bdf)
        })
        .ok_or("no xHCI controller on the PCI bus")?;

    let vendor = host.read_u16(bdf, pci::config::VENDOR_ID);
    let device = host.read_u16(bdf, pci::config::DEVICE_ID);
    let bar = host
        .setup_memory_bar(bdf, 0)
        .ok_or("the xHCI BAR0 does not fit any host memory window")?;
    host.enable_memory_and_bus_master(bdf);

    let pin = host.read_u8(bdf, pci::config::INTERRUPT_PIN);
    let irq = host.interrupt_for_device(bdf, pin);

    usb_println!(
        "xHCI PCI: {} {:04x}:{:04x} BAR0 {:#x} ({:#x} bytes, bus {:#x}) COMMAND={:04x} pin {} INT{} {:?}",
        bdf,
        vendor,
        device,
        bar.cpu_address,
        bar.size,
        bar.pci_address,
        host.read_u16(bdf, pci::config::COMMAND),
        pin,
        (b'A' + pin.saturating_sub(1).min(3)) as char,
        irq.map(|v| v.0)
    );

    Ok(XhciPciDevice {
        bdf,
        vendor,
        device,
        mmio_base: bar.cpu_address,
        mmio_size: bar.size,
        irq,
        dma: host.dma_window(),
    })
}

/// Brings the controller up, registers it as a system service and puts its
/// keyboard in front of the existing console input.
///
/// Mirrors what `rpi::usb::init` does for DWC2 on a Raspberry Pi 3, including
/// the bounded foreground window: enumeration gets a little time while the
/// UART is still the active console, and then continues in the background so
/// an absent or slow keyboard never holds up the boot.
pub unsafe fn init(tree: &fdt::DeviceTree) -> Result<(), &'static str> {
    let mut host = PciHost::probe_ecam(tree).ok_or("no ECAM PCI host bridge in the device tree")?;
    unsafe { init_with(&mut host) }
}

/// The same, on a host that has already been found and brought up — the
/// BCM2711 of a Raspberry Pi 4 or 400, whose link MiniOS trains itself.
///
/// # Safety
/// As [`init`]; additionally `host` must be ready for configuration cycles.
pub unsafe fn init_with<C: ConfigAccess>(host: &mut PciHost<C>) -> Result<(), &'static str> {
    let found = probe_on(host)?;
    if found.dma != DmaWindow::IDENTITY {
        usb_println!(
            "xHCI DMA: CPU {:#x}..{:#x} seen by the device at +{:#x}",
            found.dma.cpu_start,
            found.dma.cpu_end,
            found.dma.offset
        );
    }
    let env: &'static DtXhciEnv =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(DtXhciEnv::new(found.dma)));
    let mut controller =
        unsafe { Xhci::new(found.mmio_base, env) }.map_err(|_| "xHCI failed to start")?;

    report_capabilities(&controller);
    let max_ports = controller.capabilities().max_ports;
    match controller.no_op() {
        Ok(()) => usb_println!("xHCI self test: No-Op command completed"),
        Err(error) => {
            usb_println!("xHCI self test: No-Op command failed: {:?}", error);
            return Err("the xHCI command ring does not complete commands");
        }
    }

    let mut usb = XhciUsb::new(controller);
    usb_println!(
        "xHCI: managing USB 2.0 root ports {:#06x} of {}",
        usb.managed_ports(),
        max_ports
    );

    // Give enumeration a bounded foreground window, the same way the Pi 3
    // path does.  A keyboard present at power-on is then usable by the time
    // the menu appears; one plugged in later is picked up by the service.
    let bootstrap = counter_us();
    while !usb.keyboard_ready() && counter_us().wrapping_sub(bootstrap) < BOOTSTRAP_US {
        usb.poll_once()
            .map_err(|_| "xHCI bootstrap polling failed")?;
        core::hint::spin_loop();
    }
    report_ports(&usb);

    // Interrupts go on only once enumeration has been given its foreground
    // window: until something is draining the event ring, a level-triggered
    // line would be re-taken with nobody to answer it.
    // Ask before enabling: not every interrupt controller this kernel drives
    // can route a shared peripheral interrupt, and polling is a working
    // fallback where one cannot.
    match found.irq.filter(|line| irq::can_enable(*line)) {
        Some(line) => {
            let ack = usb.interrupt_ack();
            ACK_USBSTS.store(ack.usbsts_address(), Ordering::Relaxed);
            ACK_IMAN.store(ack.iman_address(), Ordering::Release);
            usb.enable_interrupts();
            unsafe { irq::register_usb_handler(line, interrupt_handler) };
            usb_println!("xHCI completion mode: interrupt (IRQ {})", line.0);
        }
        None => usb_println!(
            "xHCI completion mode: polling ({})",
            match found.irq {
                Some(line) => {
                    let _ = line;
                    "the interrupt controller cannot enable this line"
                }
                None => "no interrupt in the device tree",
            }
        ),
    }

    System::register_service(alloc::boxed::Box::new(usb))
        .map_err(|_| "xHCI service registration failed")?;
    let fallback = System::stdin();
    let mux = alloc::boxed::Box::leak(alloc::boxed::Box::new(UsbTextInputMux::new(fallback)));
    unsafe { System::set_stdin(mux) };
    Ok(())
}

/// How long enumeration runs in the foreground before the boot continues.
const BOOTSTRAP_US: u64 = 3_000_000;

/// Where the interrupt handler finds the controller.  Written once, before
/// the interrupt is enabled.
static ACK_USBSTS: AtomicUsize = AtomicUsize::new(0);
static ACK_IMAN: AtomicUsize = AtomicUsize::new(0);

/// Entries into the handler, and the value the foreground last saw.
static ENTRIES: AtomicU32 = AtomicU32::new(0);
static OBSERVED: AtomicU32 = AtomicU32::new(0);

/// Handler entries with no foreground run in between before the line is
/// treated as unserviceable.  The foreground runs on every system tick, so
/// anything past a burst means the interrupt is being re-taken faster than it
/// can be answered — and a wedged boot is a worse outcome than polling.
const STORM_LIMIT: u32 = 1024;

/// Minimal ISR: acknowledge and mask the interrupter.  The event ring itself
/// is drained by the service, which also re-arms the interrupter.
fn interrupt_handler() {
    let iman = ACK_IMAN.load(Ordering::Acquire);
    if iman == 0 {
        return;
    }
    let entries = ENTRIES.load(Ordering::Relaxed).wrapping_add(1);
    ENTRIES.store(entries, Ordering::Relaxed);
    if entries.wrapping_sub(OBSERVED.load(Ordering::Relaxed)) > STORM_LIMIT {
        // Nothing has drained the ring for a very long run of interrupts.
        // Leave the interrupter masked; `poll_events` re-arms it only while
        // the driver still believes interrupts work, and the service notices
        // the stall and falls back to polling.
        ACK_IMAN.store(0, Ordering::Release);
        return;
    }
    let ack = unsafe { InterruptAck::from_addresses(ACK_USBSTS.load(Ordering::Relaxed), iman) };
    unsafe { ack.acknowledge() };
}

/// True once the handler gave the line up as unserviceable.
///
/// The handler zeroes the address it needs rather than keeping a separate
/// flag, so one load answers both "is this installed" and "has it given up".
fn interrupt_stormed() -> bool {
    ACK_IMAN.load(Ordering::Acquire) == 0
}

/// Called from the foreground to show that it is still getting time.
fn note_progress() {
    let entries = ENTRIES.load(Ordering::Relaxed);
    // Say so once.  Whether the line is really wired through the interrupt
    // controller is otherwise invisible: completions keep arriving either
    // way, just with a system tick of latency instead of none.
    if entries != 0 && !FIRST_INTERRUPT_REPORTED.swap(true, Ordering::Relaxed) {
        usb_println!("xHCI: first interrupt serviced");
    }
    OBSERVED.store(entries, Ordering::Relaxed)
}

static FIRST_INTERRUPT_REPORTED: AtomicBool = AtomicBool::new(false);

fn report_capabilities<E: XhciEnv>(controller: &Xhci<'_, E>) {
    let cap = controller.capabilities();
    usb_println!(
        "xHCI {:x}.{:x}: {} ports, {} slots, {} interrupters, {}-byte contexts, {} scratchpads",
        cap.hci_version >> 8,
        (cap.hci_version >> 4) & 0xf,
        cap.max_ports,
        cap.max_slots,
        cap.max_interrupters,
        cap.context_bytes(),
        cap.max_scratchpad_buffers
    );
    usb_println!(
        "xHCI params: HCSPARAMS1={:08x} HCSPARAMS2={:08x} HCCPARAMS1={:08x} page {} xECP {:#x}",
        cap.hcsparams1,
        cap.hcsparams2,
        cap.hccparams1,
        cap.page_size,
        cap.extended_capabilities
    );
    let r = controller.register_snapshot();
    usb_println!(
        "xHCI rings: CRCR={:012x}/{:012x} DCBAAP={:012x}/{:012x} ERSTBA={:012x}/{:012x} ERDP={:012x}/{:012x} USBCMD={:08x} USBSTS={:08x}",
        r.crcr,
        r.command_ring,
        r.dcbaap,
        r.dcbaa,
        r.erstba,
        r.erst,
        r.erdp,
        r.event_ring,
        r.usbcmd,
        r.usbsts
    );
}

fn report_ports<E: XhciEnv>(usb: &XhciUsb<'_, E>) {
    let controller = usb.controller_snapshot();
    let devices = usb.snapshot();
    usb_println!(
        "xHCI: {} connect(s), {} enumerated, {} ignored, {} failure(s), keyboard {}",
        devices.connects,
        devices.enumerated,
        devices.ignored_devices,
        devices.enumeration_failures,
        if usb.keyboard_ready() {
            "ready"
        } else {
            "not found"
        }
    );
    usb_println!(
        "xHCI: events {} ({} port, {} unexpected, {} dropped), commands {} ({} errors, {} timeouts), transfers {} ({} errors, {} timeouts)",
        controller.events,
        controller.port_events,
        controller.unexpected_events,
        controller.dropped_events,
        controller.commands,
        controller.command_errors,
        controller.command_timeouts,
        controller.transfers,
        controller.transfer_errors,
        controller.transfer_timeouts
    );
    if controller.host_errors > 0 || controller.leaked_slots > 0 {
        usb_println!(
            "xHCI: {} fault(s) (last USBSTS {:08x}), {} slot(s) kept after a failed Disable Slot",
            controller.host_errors,
            controller.last_fault_status,
            controller.leaked_slots
        );
    }
}

/// Reports whether the machine might have an xHCI controller worth probing,
/// without changing anything.  Used to keep the probe — and its boot message
/// — off boards it does not apply to.
///
/// Only the root bus can be read without writing: behind a bridge nothing
/// answers until the bridge has been given bus numbers.  So a PCI-to-PCI
/// bridge on the root bus counts as a maybe, and [`init`] finds out.
pub fn is_available(tree: &fdt::DeviceTree) -> bool {
    PciHost::probe_ecam(tree).is_some_and(|host| {
        host.find_map(|host, bdf| {
            let (class, subclass, prog_if) = host.class_of(bdf);
            let xhci = class == pci::CLASS_SERIAL_USB
                && subclass == pci::SUBCLASS_USB
                && prog_if == pci::PROG_IF_XHCI;
            let bridge = host.read_u8(bdf, pci::config::HEADER_TYPE) & 0x7f == 1;
            (xhci || bridge).then_some(())
        })
        .is_some()
    })
}

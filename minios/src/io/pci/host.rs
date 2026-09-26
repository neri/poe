//! Minimal ECAM PCI host, driven from the device tree
//!
//! Only what the USB work of `docs/USB_HOST_RPI4_PLAN.md` needs: find the
//! host bridge, number and open the bridges on its root bus, hand out memory
//! BARs from the host's own windows and resolve the legacy INTx line.  It is
//! deliberately not a PCI subsystem: one tier of bridge only, no resource
//! rebalancing, no capability framework beyond [`PciHost::find_capability`],
//! and no hot-plug.
//!
//! One tier is what a Raspberry Pi 4 needs — its VL805 sits directly behind
//! the BCM2711's root port — and a bridge behind a bridge would need its
//! subordinate bus number fixed up after its children were numbered.
//!
//! Firmware may have left BARs already programmed (the Raspberry Pi bootloader
//! does this for the VL805).  An address that already falls inside a host
//! window is kept rather than reassigned, so this stays usable on a board
//! whose firmware set things up before the kernel ran.

use fdt::{PHandle, PropName};

use super::{Bdf, ConfigAccess, Ecam, Mmio};

/// Configuration space offsets used here.
pub mod config {
    pub const VENDOR_ID: u16 = 0x00;
    pub const DEVICE_ID: u16 = 0x02;
    pub const COMMAND: u16 = 0x04;
    pub const STATUS: u16 = 0x06;
    pub const REVISION_ID: u16 = 0x08;
    pub const PROG_IF: u16 = 0x09;
    pub const SUBCLASS: u16 = 0x0a;
    pub const CLASS: u16 = 0x0b;
    pub const CACHE_LINE_SIZE: u16 = 0x0c;
    pub const HEADER_TYPE: u16 = 0x0e;
    pub const BAR0: u16 = 0x10;
    /// Type 1 (bridge) header: primary, secondary, subordinate bus numbers.
    pub const PRIMARY_BUS: u16 = 0x18;
    pub const IO_BASE: u16 = 0x1c;
    pub const MEMORY_BASE: u16 = 0x20;
    pub const PREFETCHABLE_BASE: u16 = 0x24;
    pub const PREFETCHABLE_BASE_UPPER: u16 = 0x28;
    pub const PREFETCHABLE_LIMIT_UPPER: u16 = 0x2c;
    pub const CAPABILITY_POINTER: u16 = 0x34;
    pub const INTERRUPT_LINE: u16 = 0x3c;
    pub const INTERRUPT_PIN: u16 = 0x3d;
}

/// `COMMAND` register bits.
pub mod command {
    pub const IO_SPACE: u16 = 1 << 0;
    pub const MEMORY_SPACE: u16 = 1 << 1;
    pub const BUS_MASTER: u16 = 1 << 2;
    pub const INTERRUPT_DISABLE: u16 = 1 << 10;
}

/// `STATUS` register bits.
pub mod status {
    pub const CAPABILITIES_LIST: u16 = 1 << 4;
}

/// Base class 0x0c, subclass 0x03, programming interface 0x30: xHCI.
pub const CLASS_SERIAL_USB: u8 = 0x0c;
pub const SUBCLASS_USB: u8 = 0x03;
pub const PROG_IF_XHCI: u8 = 0x30;

/// One `ranges` entry of the host bridge, used as a bump allocator for BARs.
#[derive(Clone, Copy, Debug)]
struct Window {
    /// Address as the device sees it.
    pci_base: u64,
    /// Address as the CPU sees it.
    cpu_base: u64,
    len: u64,
    /// Next free offset into the window.
    next: u64,
}

impl Window {
    fn contains_pci(&self, base: u64, len: u64) -> bool {
        base >= self.pci_base && len <= self.len && base - self.pci_base <= self.len - len
    }

    /// Reserves `len` bytes aligned to `len` (BARs are naturally aligned).
    fn allocate(&mut self, len: u64) -> Option<u64> {
        let align = len.max(0x1000);
        let start = (self.next + align - 1) & !(align - 1);
        if start.checked_add(len)? > self.len {
            return None;
        }
        self.next = start + len;
        Some(self.pci_base + start)
    }

    fn to_cpu(&self, pci: u64) -> Option<usize> {
        if pci < self.pci_base || pci - self.pci_base >= self.len {
            return None;
        }
        usize::try_from(self.cpu_base + (pci - self.pci_base)).ok()
    }
}

/// A mapped memory BAR.
#[derive(Clone, Copy, Debug)]
pub struct Bar {
    /// Address programmed into the BAR, as the device sees it.
    pub pci_address: u64,
    /// The same region as the CPU addresses it.
    pub cpu_address: usize,
    pub size: u64,
    pub is_64bit: bool,
    pub prefetchable: bool,
}

/// One entry of the host bridge's `interrupt-map`.
#[derive(Clone, Copy, Default)]
struct IrqMapEntry {
    child_hi: u32,
    pin: u32,
    irq: u32,
}

/// Buses this will track: the root bus plus one per bridge on it.
const MAX_BUSES: usize = 8;

/// A bus this has found, and the bridge that leads to it.
#[derive(Clone, Copy, Debug, Default)]
struct BusEntry {
    number: u8,
    /// `None` for the root bus.
    bridge: Option<Bdf>,
}

/// Where a host bridge lets its devices DMA, from its `dma-ranges`.
///
/// A device's DMA address is the CPU address plus `offset`, and only CPU
/// memory in `cpu_start..cpu_end` is reachable at all.  On a Raspberry Pi 4
/// with current firmware the whole of RAM appears to the device 16 GiB higher
/// than the CPU sees it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmaWindow {
    pub offset: u64,
    pub cpu_start: u64,
    pub cpu_end: u64,
}

impl DmaWindow {
    /// No `dma-ranges`: the device sees memory where the CPU does.
    pub const IDENTITY: Self = Self {
        offset: 0,
        cpu_start: 0,
        cpu_end: u64::MAX,
    };
}

/// The number of `interrupt-map` entries kept.  The QEMU virt machine has one
/// per (slot, pin) of the first four slots; a host with more is truncated and
/// the devices past it fall back to polling.
const MAX_IRQ_MAP: usize = 64;

/// A PCI host bridge, whatever way it reaches configuration space.
///
/// Everything above configuration access — bridge numbering, BAR sizing,
/// interrupt routing, the windows from the device tree — is the same for every
/// host this drives.  Only `C` differs: ECAM on the QEMU virt machine, the
/// BCM2711's root-direct/indexed scheme on a Raspberry Pi 4 or 400.
pub struct PciHost<C: ConfigAccess> {
    access: C,
    bus_min: u8,
    bus_max: u8,
    mem32: Option<Window>,
    mem64: Option<Window>,
    irq_map: [IrqMapEntry; MAX_IRQ_MAP],
    irq_map_len: usize,
    irq_mask_hi: u32,
    irq_mask_pin: u32,
    buses: [BusEntry; MAX_BUSES],
    bus_count: usize,
    /// The next bus number to hand to a bridge.
    next_bus: u8,
    dma: DmaWindow,
    /// `dma-coherent` on the host bridge: DMA needs no cache maintenance.
    pub dma_coherent: bool,
}

impl PciHost<Ecam<Mmio>> {
    /// Host bridges reached through ECAM.  `pci-host-ecam-generic` is what the
    /// QEMU virt machine exposes.
    const ECAM_COMPATIBLE: &'static [&'static str] = &["pci-host-ecam-generic"];

    /// Finds the first usable ECAM host bridge in the device tree.
    pub fn probe_ecam(tree: &fdt::DeviceTree) -> Option<Self> {
        let (node, base, size) = fdt::bus::find_map(tree, |node, map| {
            node.is_compatible_with_any(Self::ECAM_COMPATIBLE)
                .then(|| {
                    let (base, size) = map.reg(node, 0)?;
                    Some((node.clone(), base, size))
                })
                .flatten()
        })?;
        let (bus_min, bus_max) = bus_range(&node);
        let access = Ecam::new(
            unsafe { Mmio::new(usize::try_from(base).ok()?, usize::try_from(size).ok()?) },
            bus_min,
            bus_max,
        );
        Self::from_node(tree, &node, access)
    }
}

/// `bus-range` of a host bridge.  An absent one means the whole of 0..=255,
/// which is what the BCM2711 node relies on.
pub fn bus_range(node: &fdt::Node) -> (u8, u8) {
    match node.get_prop(PropName("bus-range")).map(|v| v.words()) {
        Some([min, max, ..]) => (min.as_u32().min(255) as u8, max.as_u32().min(255) as u8),
        _ => (0, 255),
    }
}

impl<C: ConfigAccess> PciHost<C> {
    /// Builds a host from its device-tree node and a way into its
    /// configuration space.  Returns `None` if the node describes no memory
    /// window to put BARs in.
    pub fn from_node(tree: &fdt::DeviceTree, node: &fdt::Node, access: C) -> Option<Self> {
        let (bus_min, bus_max) = bus_range(node);
        let mut host = Self {
            access,
            bus_min,
            bus_max,
            mem32: None,
            mem64: None,
            irq_map: [IrqMapEntry::default(); MAX_IRQ_MAP],
            irq_map_len: 0,
            irq_mask_hi: 0,
            irq_mask_pin: 0,
            buses: [BusEntry::default(); MAX_BUSES],
            bus_count: 1,
            next_bus: bus_min.saturating_add(1),
            dma: DmaWindow::IDENTITY,
            dma_coherent: node.get_prop(PropName::DMA_COHERENT).is_some(),
        };
        host.buses[0] = BusEntry {
            number: bus_min,
            bridge: None,
        };
        host.parse_ranges(node);
        host.parse_dma_ranges(node);
        host.parse_interrupt_map(tree, node);
        (host.mem32.is_some() || host.mem64.is_some()).then_some(host)
    }

    /// The configuration space accessor, for a caller that needs it directly.
    pub fn access(&self) -> &C {
        &self.access
    }

    /// `ranges` of a PCI host is `<pci-addr(3) parent-addr(2) size(2)>`.  The
    /// high cell of the PCI address carries the space type in bits 24..25 and
    /// the prefetchable flag in bit 30.  `fdt::Node::ranges` cannot be used:
    /// it rejects `#address-cells = <3>`.
    fn parse_ranges(&mut self, node: &fdt::Node) {
        let Some(prop) = node.get_prop(PropName::RANGES) else {
            return;
        };
        let words = prop.words();
        for entry in words.chunks_exact(7) {
            let flags = entry[0].as_u32();
            let pci_base = ((entry[1].as_u32() as u64) << 32) | entry[2].as_u32() as u64;
            let cpu_base = ((entry[3].as_u32() as u64) << 32) | entry[4].as_u32() as u64;
            let len = ((entry[5].as_u32() as u64) << 32) | entry[6].as_u32() as u64;
            if len == 0 {
                continue;
            }
            let window = Window {
                pci_base,
                cpu_base,
                len,
                next: 0,
            };
            match (flags >> 24) & 3 {
                // 32-bit memory space
                2 => self.mem32.get_or_insert(window),
                // 64-bit memory space
                3 => self.mem64.get_or_insert(window),
                // I/O space and configuration space are not used
                _ => continue,
            };
        }
    }

    /// `dma-ranges` of a PCI host has the same `<pci(3) cpu(2) size(2)>`
    /// layout as `ranges`, read the other way: the PCI address is where the
    /// device must aim to reach the CPU address.  Only the first memory entry
    /// is used, which is all a host this drives has.
    fn parse_dma_ranges(&mut self, node: &fdt::Node) {
        let Some(prop) = node.get_prop(PropName::DMA_RANGES) else {
            return;
        };
        let Some(entry) = prop.words().chunks_exact(7).next() else {
            return;
        };
        let pci = ((entry[1].as_u32() as u64) << 32) | entry[2].as_u32() as u64;
        let cpu = ((entry[3].as_u32() as u64) << 32) | entry[4].as_u32() as u64;
        let len = ((entry[5].as_u32() as u64) << 32) | entry[6].as_u32() as u64;
        if len == 0 {
            return;
        }
        self.dma = DmaWindow {
            offset: pci.wrapping_sub(cpu),
            cpu_start: cpu,
            cpu_end: cpu.saturating_add(len),
        };
    }

    /// The host's 32-bit memory window as `(pci, cpu, len)`, which a host
    /// whose outbound window is not preset (the BCM2711) has to program.
    pub fn mem32_window(&self) -> Option<(u64, u64, u64)> {
        self.mem32.map(|w| (w.pci_base, w.cpu_base, w.len))
    }

    /// Where devices behind this host may DMA.
    pub fn dma_window(&self) -> DmaWindow {
        self.dma
    }

    /// `interrupt-map` of a PCI host is a list of
    /// `<child-unit-address child-interrupt parent-phandle parent-unit-address
    /// parent-interrupt>`.  The widths come from four different places: three
    /// cells of child address (PCI is always `#address-cells = <3>`), the
    /// host's own `#interrupt-cells`, and then the *parent's*
    /// `#address-cells` and `#interrupt-cells`.
    ///
    /// The parent unit address is the one that is easy to miss.  A GIC
    /// declares `#address-cells = <2>`, so every entry carries two zero cells
    /// between the phandle and the interrupt specifier; skipping them shifts
    /// the whole walk and no device ever matches.
    ///
    /// Only entries whose parent is a GIC (`<type number flags>`) are kept.
    fn parse_interrupt_map(&mut self, tree: &fdt::DeviceTree, node: &fdt::Node) {
        let Some(mask) = node
            .get_prop(PropName::INTERRUPT_MAP_MASK)
            .map(|v| v.words())
        else {
            return;
        };
        let Some(map) = node.get_prop(PropName::INTERRUPT_MAP).map(|v| v.words()) else {
            return;
        };
        let [mask_hi, _, _, mask_pin, ..] = mask else {
            return;
        };
        // A PCI host describes its children with one interrupt cell, the INTx
        // pin.  Anything else is not a binding this understands.
        if node.get_prop_u32(PropName::INTERRUPT_CELLS) != Some(1) {
            return;
        }
        self.irq_mask_hi = mask_hi.as_u32();
        self.irq_mask_pin = mask_pin.as_u32();

        const CHILD_CELLS: usize = 3 + 1;
        let mut index = 0;
        while index + CHILD_CELLS + 1 <= map.len() && self.irq_map_len < MAX_IRQ_MAP {
            let child_hi = map[index].as_u32();
            let pin = map[index + 3].as_u32();
            let phandle = PHandle(map[index + 4].as_u32());
            let Some(parent) = tree.find_by_phandle(phandle) else {
                break;
            };
            let parent_address_cells =
                parent.get_prop_u32(PropName::ADDRESS_CELLS).unwrap_or(0) as usize;
            let parent_interrupt_cells =
                parent.get_prop_u32(PropName::INTERRUPT_CELLS).unwrap_or(3) as usize;
            index += CHILD_CELLS + 1 + parent_address_cells;
            if index + parent_interrupt_cells > map.len() {
                break;
            }
            // GIC: <type number flags>, type 0 = SPI (base 32), 1 = PPI (base 16)
            if parent_interrupt_cells >= 2 {
                let kind = map[index].as_u32();
                let number = map[index + 1].as_u32();
                if let Some(irq) = match kind {
                    0 => Some(32 + number),
                    1 => Some(16 + number),
                    _ => None,
                } {
                    self.irq_map[self.irq_map_len] = IrqMapEntry { child_hi, pin, irq };
                    self.irq_map_len += 1;
                }
            }
            index += parent_interrupt_cells;
        }
    }

    /// The legacy interrupt of a device anywhere below the host, swizzling its
    /// pin through each bridge on the way up.
    ///
    /// A bridge rotates INTx by the device number on its secondary side, so
    /// INTA of device 1 arrives at the bridge as INTB.  `interrupt-map` only
    /// describes the root bus, so the lookup happens with the pin and address
    /// the root bus actually sees.
    pub fn interrupt_for_device(&self, bdf: Bdf, pin: u8) -> Option<u32> {
        if pin == 0 || pin > 4 {
            return None;
        }
        let (mut bdf, mut pin) = (bdf, pin);
        for _ in 0..MAX_BUSES {
            let Some(bridge) = self.bridge_of(bdf.bus) else {
                return self.interrupt_for(bdf, pin);
            };
            pin = ((pin - 1 + bdf.device) % 4) + 1;
            bdf = bridge;
        }
        None
    }

    fn bridge_of(&self, bus: u8) -> Option<Bdf> {
        self.buses[..self.bus_count]
            .iter()
            .find(|entry| entry.number == bus)
            .and_then(|entry| entry.bridge)
    }

    /// The legacy interrupt of `bdf` on `pin` (1 = INTA), if the tree maps it.
    /// `bdf` must be on the root bus; see [`Self::interrupt_for_device`].
    pub fn interrupt_for(&self, bdf: Bdf, pin: u8) -> Option<u32> {
        if pin == 0 || self.irq_map_len == 0 {
            return None;
        }
        let want_hi = bdf.interrupt_map_address() & self.irq_mask_hi;
        let want_pin = (pin as u32) & self.irq_mask_pin;
        self.irq_map[..self.irq_map_len]
            .iter()
            .find(|e| {
                e.child_hi & self.irq_mask_hi == want_hi && e.pin & self.irq_mask_pin == want_pin
            })
            .map(|e| e.irq)
    }

    /// Configuration space is little endian; an absent function reads as all
    /// ones.  Buses outside this host's range are refused here as well as by
    /// the accessor, so the two cannot disagree about what exists.
    pub fn read_u32(&self, bdf: Bdf, offset: u16) -> u32 {
        if bdf.bus < self.bus_min || bdf.bus > self.bus_max {
            return u32::MAX;
        }
        self.access.read_u32(bdf, offset)
    }

    pub fn write_u32(&self, bdf: Bdf, offset: u16, value: u32) {
        if bdf.bus < self.bus_min || bdf.bus > self.bus_max {
            return;
        }
        self.access.write_u32(bdf, offset, value)
    }

    pub fn read_u16(&self, bdf: Bdf, offset: u16) -> u16 {
        (self.read_u32(bdf, offset) >> ((offset & 2) * 8)) as u16
    }

    pub fn write_u16(&self, bdf: Bdf, offset: u16, value: u16) {
        let shift = (offset & 2) * 8;
        let mut word = self.read_u32(bdf, offset);
        word &= !(0xffff << shift);
        word |= (value as u32) << shift;
        self.write_u32(bdf, offset, word);
    }

    pub fn read_u8(&self, bdf: Bdf, offset: u16) -> u8 {
        (self.read_u32(bdf, offset) >> ((offset & 3) * 8)) as u8
    }

    pub fn write_u8(&self, bdf: Bdf, offset: u16, value: u8) {
        let shift = (offset & 3) * 8;
        let mut word = self.read_u32(bdf, offset);
        word &= !(0xff << shift);
        word |= (value as u32) << shift;
        self.write_u32(bdf, offset, word);
    }

    fn exists(&self, bdf: Bdf) -> bool {
        let vendor = self.read_u16(bdf, config::VENDOR_ID);
        vendor != 0xffff && vendor != 0x0000
    }

    /// Calls `f` for each function on every bus found so far, until it
    /// returns `Some`.  Call [`Self::configure_bridges`] first to see what is
    /// behind a root port.
    pub fn find_map<T>(&self, mut f: impl FnMut(&Self, Bdf) -> Option<T>) -> Option<T> {
        for entry in self.buses[..self.bus_count].iter() {
            if let Some(found) = self.find_map_on(entry.number, &mut f) {
                return Some(found);
            }
        }
        None
    }

    fn find_map_on<T>(&self, bus: u8, f: &mut impl FnMut(&Self, Bdf) -> Option<T>) -> Option<T> {
        for device in 0..32u8 {
            let first = Bdf::new(bus, device, 0);
            if !self.exists(first) {
                continue;
            }
            let multi_function = self.read_u8(first, config::HEADER_TYPE) & 0x80 != 0;
            let functions = if multi_function { 8 } else { 1 };
            for function in 0..functions {
                let bdf = Bdf::new(bus, device, function);
                if !self.exists(bdf) {
                    continue;
                }
                if let Some(found) = f(self, bdf) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Numbers every PCI-to-PCI bridge on the root bus and opens its memory
    /// window, so the devices behind it answer configuration cycles and their
    /// BARs are reachable.  Returns how many bridges were set up by this call;
    /// a bridge already numbered is left alone, so calling it again is safe.
    ///
    /// Each bridge forwards the host's whole 32-bit memory window.  That is
    /// only correct because BARs are handed out from that one window and there
    /// is one tier: with two bridges side by side it still works, since both
    /// forward everything and each device claims only its own BAR.
    pub fn configure_bridges(&mut self) -> usize {
        let root = self.buses[0].number;
        let mut bridges: [Option<Bdf>; MAX_BUSES] = [None; MAX_BUSES];
        let mut count = 0;
        self.find_map_on(root, &mut |host, bdf| {
            let known = host.buses[..host.bus_count]
                .iter()
                .any(|entry| entry.bridge == Some(bdf));
            if !known && count < MAX_BUSES && host.read_u8(bdf, config::HEADER_TYPE) & 0x7f == 1 {
                bridges[count] = Some(bdf);
                count += 1;
            }
            None::<()>
        });
        let mut configured = 0;
        for bridge in bridges.into_iter().flatten() {
            if self.configure_bridge(root, bridge) {
                configured += 1;
            }
        }
        configured
    }

    fn configure_bridge(&mut self, primary: u8, bridge: Bdf) -> bool {
        if self.bus_count >= MAX_BUSES || self.next_bus > self.bus_max {
            return false;
        }
        let secondary = self.next_bus;
        self.next_bus += 1;

        // Keep the secondary latency timer in the top byte; the three bus
        // numbers below it are what route configuration cycles.  With one
        // tier the subordinate bus is the secondary bus itself.
        let latency = self.read_u32(bridge, config::PRIMARY_BUS) & 0xff00_0000;
        self.write_u32(
            bridge,
            config::PRIMARY_BUS,
            latency | primary as u32 | (secondary as u32) << 8 | (secondary as u32) << 16,
        );

        // Memory window: base and limit are bits 31..20 of the address, in the
        // top twelve bits of each half.  The limit is inclusive.
        if let Some(window) = self.mem32 {
            let start = window.pci_base;
            let end = window.pci_base + window.len - 1;
            let base = ((start >> 16) & 0xfff0) as u32;
            let limit = ((end >> 16) & 0xfff0) as u32;
            self.write_u32(bridge, config::MEMORY_BASE, base | (limit << 16));
        }
        // Close the prefetchable and I/O windows by putting their base above
        // their limit.  Nothing here allocates from them, and an open window
        // left at reset values could forward something unintended.
        self.write_u32(bridge, config::PREFETCHABLE_BASE, 0x0000_fff0);
        self.write_u32(bridge, config::PREFETCHABLE_BASE_UPPER, 0);
        self.write_u32(bridge, config::PREFETCHABLE_LIMIT_UPPER, 0);
        self.write_u16(bridge, config::IO_BASE, 0x00f0);

        self.enable_memory_and_bus_master(bridge);
        self.buses[self.bus_count] = BusEntry {
            number: secondary,
            bridge: Some(bridge),
        };
        self.bus_count += 1;
        true
    }

    /// The buses found so far, root bus first.
    pub fn bus_numbers(&self) -> impl Iterator<Item = u8> + '_ {
        self.buses[..self.bus_count]
            .iter()
            .map(|entry| entry.number)
    }

    /// The class triple `(class, subclass, programming interface)` of `bdf`.
    pub fn class_of(&self, bdf: Bdf) -> (u8, u8, u8) {
        (
            self.read_u8(bdf, config::CLASS),
            self.read_u8(bdf, config::SUBCLASS),
            self.read_u8(bdf, config::PROG_IF),
        )
    }

    /// Reads, sizes and if necessary assigns the memory BAR at `index`.
    ///
    /// Returns `None` for an I/O BAR, an unimplemented BAR, or one that does
    /// not fit any host window.  Memory decoding must be off while the BAR is
    /// sized, so the caller enables it afterwards.
    pub fn setup_memory_bar(&mut self, bdf: Bdf, index: u8) -> Option<Bar> {
        if index >= 6 {
            return None;
        }
        // Memory decoding stays off for the whole of this: sizing puts an
        // address the host does not route into the BAR, and so does a
        // half-written 64-bit address.  Every exit below restores it.
        let command = self.read_u16(bdf, config::COMMAND);
        self.write_u16(
            bdf,
            config::COMMAND,
            command & !(command::MEMORY_SPACE | command::IO_SPACE),
        );
        let bar = self.setup_memory_bar_inner(bdf, index);
        self.write_u16(bdf, config::COMMAND, command);
        bar
    }

    fn setup_memory_bar_inner(&mut self, bdf: Bdf, index: u8) -> Option<Bar> {
        let offset = config::BAR0 + index as u16 * 4;
        let original = self.read_u32(bdf, offset);
        if original & 1 != 0 {
            // I/O space
            return None;
        }
        let is_64bit = (original >> 1) & 3 == 2;
        let prefetchable = original & (1 << 3) != 0;
        if is_64bit && index >= 5 {
            return None;
        }
        let original_hi = is_64bit.then(|| self.read_u32(bdf, offset + 4));

        // A BAR reports its size as the bits it refuses to keep: write all
        // ones, read back, and the lowest set bit is the alignment.
        self.write_u32(bdf, offset, u32::MAX);
        let probe_lo = self.read_u32(bdf, offset);
        let probe_hi = if is_64bit {
            self.write_u32(bdf, offset + 4, u32::MAX);
            self.read_u32(bdf, offset + 4)
        } else {
            u32::MAX
        };
        self.write_u32(bdf, offset, original);
        if let Some(hi) = original_hi {
            self.write_u32(bdf, offset + 4, hi);
        }

        let mask = ((probe_hi as u64) << 32) | (probe_lo & !0xf) as u64;
        // An unimplemented BAR reads back as all zeros, which is not a size.
        if mask == u64::MAX || probe_lo & !0xf == 0 && !is_64bit {
            return None;
        }
        let size = (!mask).wrapping_add(1);
        if size == 0 {
            return None;
        }

        let current = ((original_hi.unwrap_or(0) as u64) << 32) | (original & !0xf) as u64;
        let (pci_address, window) = self.pick_window(current, size, is_64bit, prefetchable)?;
        let cpu_address = window.to_cpu(pci_address)?;

        self.write_u32(bdf, offset, (pci_address as u32 & !0xf) | (original & 0xf));
        if is_64bit {
            self.write_u32(bdf, offset + 4, (pci_address >> 32) as u32);
        }

        Some(Bar {
            pci_address,
            cpu_address,
            size,
            is_64bit,
            prefetchable,
        })
    }

    /// Keeps an address firmware already programmed when it lies in a host
    /// window, and otherwise takes the next free slot of a suitable window.
    fn pick_window(
        &mut self,
        current: u64,
        size: u64,
        is_64bit: bool,
        _prefetchable: bool,
    ) -> Option<(u64, Window)> {
        if current != 0 {
            if let Some(w) = self.mem32.filter(|w| w.contains_pci(current, size)) {
                return Some((current, w));
            }
            if is_64bit && let Some(w) = self.mem64.filter(|w| w.contains_pci(current, size)) {
                return Some((current, w));
            }
        }
        if let Some(window) = self.mem32.as_mut()
            && let Some(address) = window.allocate(size)
        {
            return Some((address, *window));
        }
        if is_64bit
            && let Some(window) = self.mem64.as_mut()
            && let Some(address) = window.allocate(size)
        {
            return Some((address, *window));
        }
        None
    }

    /// Turns on memory decoding and bus mastering, and leaves INTx enabled.
    pub fn enable_memory_and_bus_master(&self, bdf: Bdf) {
        let mut command = self.read_u16(bdf, config::COMMAND);
        command |= command::MEMORY_SPACE | command::BUS_MASTER;
        command &= !command::INTERRUPT_DISABLE;
        self.write_u16(bdf, config::COMMAND, command);
        // A cache line size of 0 is legal but makes some devices fall back to
        // single-beat transfers.  64 bytes matches every host this runs on.
        if self.read_u8(bdf, config::CACHE_LINE_SIZE) == 0 {
            self.write_u8(bdf, config::CACHE_LINE_SIZE, 16);
        }
    }

    /// Offset of the first capability with `id`, walking the capability list.
    pub fn find_capability(&self, bdf: Bdf, id: u8) -> Option<u16> {
        if self.read_u16(bdf, config::STATUS) & status::CAPABILITIES_LIST == 0 {
            return None;
        }
        let mut offset = self.read_u8(bdf, config::CAPABILITY_POINTER) as u16 & !3;
        // The list is bounded by the size of configuration space; a loop in a
        // broken list would otherwise never end.
        for _ in 0..48 {
            if offset < 0x40 || offset >= 0x100 {
                return None;
            }
            let header = self.read_u16(bdf, offset);
            if header as u8 == id {
                return Some(offset);
            }
            offset = (header >> 8) as u16 & !3;
        }
        None
    }

    pub fn bus_range(&self) -> (u8, u8) {
        (self.bus_min, self.bus_max)
    }
}

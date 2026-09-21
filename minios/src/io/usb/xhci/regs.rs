//! xHCI register windows.
//!
//! Offsets follow the xHCI specification revision 1.2.  The capability
//! registers are at the base of the BAR; the operational, runtime and doorbell
//! windows are found through `CAPLENGTH`, `RTSOFF` and `DBOFF`.
//!
//! Every access here is volatile.  64-bit registers are written as two 32-bit
//! halves, low half first.  A single 64-bit store is not guaranteed to reach
//! the controller as one transaction, and the order matters: a controller
//! latches a pointer register when its *high* half is written, so writing the
//! high half first hands it an address whose low half is still stale.  QEMU's
//! xHCI does exactly that, and a command ring programmed the other way around
//! is fetched from address zero.

/// Capability register offsets.
pub mod cap {
    pub const CAPLENGTH: usize = 0x00;
    pub const HCIVERSION: usize = 0x02;
    pub const HCSPARAMS1: usize = 0x04;
    pub const HCSPARAMS2: usize = 0x08;
    pub const HCSPARAMS3: usize = 0x0c;
    pub const HCCPARAMS1: usize = 0x10;
    pub const DBOFF: usize = 0x14;
    pub const RTSOFF: usize = 0x18;
    pub const HCCPARAMS2: usize = 0x1c;
}

/// Operational register offsets, relative to the operational base.
pub mod op {
    pub const USBCMD: usize = 0x00;
    pub const USBSTS: usize = 0x04;
    pub const PAGESIZE: usize = 0x08;
    pub const DNCTRL: usize = 0x14;
    pub const CRCR: usize = 0x18;
    pub const DCBAAP: usize = 0x30;
    pub const CONFIG: usize = 0x38;
    pub const PORT_BASE: usize = 0x400;
    pub const PORT_STRIDE: usize = 0x10;
    /// Port register offsets within one port's block.
    pub const PORTSC: usize = 0x00;
}

/// `USBCMD` bits.
pub mod usbcmd {
    pub const RUN: u32 = 1 << 0;
    pub const HOST_CONTROLLER_RESET: u32 = 1 << 1;
    pub const INTERRUPTER_ENABLE: u32 = 1 << 2;
    pub const HOST_SYSTEM_ERROR_ENABLE: u32 = 1 << 3;
}

/// `USBSTS` bits.
pub mod usbsts {
    pub const HALTED: u32 = 1 << 0;
    pub const HOST_SYSTEM_ERROR: u32 = 1 << 2;
    pub const EVENT_INTERRUPT: u32 = 1 << 3;
    pub const PORT_CHANGE_DETECT: u32 = 1 << 4;
    pub const SAVE_STATE_STATUS: u32 = 1 << 8;
    pub const RESTORE_STATE_STATUS: u32 = 1 << 9;
    pub const SAVE_RESTORE_ERROR: u32 = 1 << 10;
    pub const CONTROLLER_NOT_READY: u32 = 1 << 11;
    pub const HOST_CONTROLLER_ERROR: u32 = 1 << 12;
    /// The write-1-to-clear subset, used to acknowledge without disturbing
    /// the rest.  `HCE` is deliberately absent: it is read-only and stays set
    /// until the controller is reset, which is the correct behaviour — a
    /// controller that has faulted is not made well by acknowledging it.
    pub const RW1C_MASK: u32 =
        HOST_SYSTEM_ERROR | EVENT_INTERRUPT | PORT_CHANGE_DETECT | SAVE_RESTORE_ERROR;
}

/// `CRCR` bits.  The pointer occupies bits 63..6.
pub mod crcr {
    pub const RING_CYCLE_STATE: u64 = 1 << 0;
    pub const COMMAND_STOP: u64 = 1 << 1;
    pub const COMMAND_ABORT: u64 = 1 << 2;
    pub const COMMAND_RING_RUNNING: u64 = 1 << 3;
}

/// `PORTSC` bits.
pub mod portsc {
    pub const CURRENT_CONNECT_STATUS: u32 = 1 << 0;
    pub const PORT_ENABLED: u32 = 1 << 1;
    pub const OVER_CURRENT_ACTIVE: u32 = 1 << 3;
    pub const PORT_RESET: u32 = 1 << 4;
    pub const PORT_POWER: u32 = 1 << 9;
    pub const CONNECT_STATUS_CHANGE: u32 = 1 << 17;
    pub const PORT_ENABLED_CHANGE: u32 = 1 << 18;
    pub const WARM_RESET_CHANGE: u32 = 1 << 19;
    pub const OVER_CURRENT_CHANGE: u32 = 1 << 20;
    pub const PORT_RESET_CHANGE: u32 = 1 << 21;
    pub const PORT_LINK_STATE_CHANGE: u32 = 1 << 22;
    pub const CONFIG_ERROR_CHANGE: u32 = 1 << 23;

    /// Every write-1-to-clear change bit.
    pub const CHANGE_MASK: u32 = CONNECT_STATUS_CHANGE
        | PORT_ENABLED_CHANGE
        | WARM_RESET_CHANGE
        | OVER_CURRENT_CHANGE
        | PORT_RESET_CHANGE
        | PORT_LINK_STATE_CHANGE
        | CONFIG_ERROR_CHANGE;

    /// Bits that must be written as zero to leave the port alone.  `PED` is
    /// write-1-to-clear and *disables* the port, so a read-modify-write that
    /// keeps it set would drop the device.
    pub const PRESERVE_MASK: u32 = !(CHANGE_MASK | PORT_ENABLED | PORT_RESET | (1 << 31));

    #[inline]
    pub const fn link_state(value: u32) -> u8 {
        ((value >> 5) & 0xf) as u8
    }

    #[inline]
    pub const fn port_speed(value: u32) -> u8 {
        ((value >> 10) & 0xf) as u8
    }
}

/// Interrupter register offsets, relative to the runtime base.
pub mod runtime {
    pub const MFINDEX: usize = 0x00;
    pub const INTERRUPTER_BASE: usize = 0x20;
    pub const INTERRUPTER_STRIDE: usize = 0x20;
    pub const IMAN: usize = 0x00;
    pub const IMOD: usize = 0x04;
    pub const ERSTSZ: usize = 0x08;
    pub const ERSTBA: usize = 0x10;
    pub const ERDP: usize = 0x18;
}

/// `IMAN` bits.
pub mod iman {
    pub const INTERRUPT_PENDING: u32 = 1 << 0;
    pub const INTERRUPT_ENABLE: u32 = 1 << 1;
}

/// `ERDP` bits.  The pointer occupies bits 63..4.
pub mod erdp {
    pub const EVENT_HANDLER_BUSY: u64 = 1 << 3;
}

/// The capability, operational, runtime and doorbell windows of one controller.
#[derive(Clone, Copy, Debug)]
pub struct Regs {
    base: usize,
    operational: usize,
    runtime: usize,
    doorbell: usize,
    max_ports: u8,
}

impl Regs {
    /// # Safety
    /// `base` must be the mapped, device-memory BAR of an xHCI controller,
    /// and must stay mapped for as long as this value is used.
    pub unsafe fn new(base: usize) -> Self {
        let mut regs = Self {
            base,
            operational: base,
            runtime: base,
            doorbell: base,
            max_ports: 0,
        };
        let caplength = unsafe { regs.read_cap_u32(cap::CAPLENGTH) } & 0xff;
        regs.operational = base + caplength as usize;
        regs.runtime = base + (unsafe { regs.read_cap_u32(cap::RTSOFF) } & !0x1f) as usize;
        regs.doorbell = base + (unsafe { regs.read_cap_u32(cap::DBOFF) } & !0x3) as usize;
        regs.max_ports = ((unsafe { regs.read_cap_u32(cap::HCSPARAMS1) } >> 24) & 0xff) as u8;
        regs
    }

    #[inline]
    pub fn base(&self) -> usize {
        self.base
    }

    #[inline]
    pub fn max_ports(&self) -> u8 {
        self.max_ports
    }

    #[inline]
    unsafe fn read32(address: usize) -> u32 {
        unsafe { (address as *const u32).read_volatile() }
    }

    #[inline]
    unsafe fn write32(address: usize, value: u32) {
        unsafe { (address as *mut u32).write_volatile(value) }
    }

    #[inline]
    pub unsafe fn read_cap_u32(&self, offset: usize) -> u32 {
        unsafe { Self::read32(self.base + offset) }
    }

    /// `HCIVERSION` is a 16-bit register at an offset of 2; it is read as part
    /// of the 32-bit word that also holds `CAPLENGTH`.
    #[inline]
    pub unsafe fn hci_version(&self) -> u16 {
        (unsafe { self.read_cap_u32(cap::CAPLENGTH) } >> 16) as u16
    }

    #[inline]
    pub unsafe fn read_op_u32(&self, offset: usize) -> u32 {
        unsafe { Self::read32(self.operational + offset) }
    }

    #[inline]
    pub unsafe fn write_op_u32(&self, offset: usize, value: u32) {
        unsafe { Self::write32(self.operational + offset, value) }
    }

    #[inline]
    pub unsafe fn read_op_u64(&self, offset: usize) -> u64 {
        unsafe {
            let lo = Self::read32(self.operational + offset) as u64;
            let hi = Self::read32(self.operational + offset + 4) as u64;
            (hi << 32) | lo
        }
    }

    #[inline]
    pub unsafe fn write_op_u64(&self, offset: usize, value: u64) {
        unsafe {
            Self::write32(self.operational + offset, value as u32);
            Self::write32(self.operational + offset + 4, (value >> 32) as u32);
        }
    }

    /// `PORTSC` of port `port` (1-based, as the specification numbers ports).
    #[inline]
    pub unsafe fn read_portsc(&self, port: u8) -> u32 {
        unsafe { Self::read32(self.port_address(port)) }
    }

    #[inline]
    pub unsafe fn write_portsc(&self, port: u8, value: u32) {
        unsafe { Self::write32(self.port_address(port), value) }
    }

    #[inline]
    fn port_address(&self, port: u8) -> usize {
        debug_assert!(port >= 1 && port <= self.max_ports);
        self.operational + op::PORT_BASE + (port as usize - 1) * op::PORT_STRIDE + op::PORTSC
    }

    /// The address of an operational register, for a handler that must reach
    /// it without a `&Xhci` to hand.
    #[inline]
    pub fn op_address(&self, offset: usize) -> usize {
        self.operational + offset
    }

    /// The address of an interrupter register, likewise.
    #[inline]
    pub fn interrupter_address(&self, index: u16, offset: usize) -> usize {
        self.interrupter(index) + offset
    }

    #[inline]
    fn interrupter(&self, index: u16) -> usize {
        self.runtime + runtime::INTERRUPTER_BASE + index as usize * runtime::INTERRUPTER_STRIDE
    }

    #[inline]
    pub unsafe fn read_interrupter_u32(&self, index: u16, offset: usize) -> u32 {
        unsafe { Self::read32(self.interrupter(index) + offset) }
    }

    #[inline]
    pub unsafe fn write_interrupter_u32(&self, index: u16, offset: usize, value: u32) {
        unsafe { Self::write32(self.interrupter(index) + offset, value) }
    }

    #[inline]
    pub unsafe fn read_interrupter_u64(&self, index: u16, offset: usize) -> u64 {
        unsafe {
            let lo = Self::read32(self.interrupter(index) + offset) as u64;
            let hi = Self::read32(self.interrupter(index) + offset + 4) as u64;
            (hi << 32) | lo
        }
    }

    #[inline]
    pub unsafe fn write_interrupter_u64(&self, index: u16, offset: usize, value: u64) {
        unsafe {
            Self::write32(self.interrupter(index) + offset, value as u32);
            Self::write32(self.interrupter(index) + offset + 4, (value >> 32) as u32);
        }
    }

    #[inline]
    pub unsafe fn read_mfindex(&self) -> u32 {
        unsafe { Self::read32(self.runtime + runtime::MFINDEX) }
    }

    /// Rings the doorbell of `slot` (0 = the command ring) with `target`
    /// (the endpoint's device context index, or 0 for a command).
    #[inline]
    pub unsafe fn ring_doorbell(&self, slot: u8, target: u8) {
        unsafe { Self::write32(self.doorbell + slot as usize * 4, target as u32) }
    }
}

/// The capability parameters the driver acts on.
#[derive(Clone, Copy, Debug, Default)]
pub struct Capabilities {
    pub hci_version: u16,
    pub max_slots: u8,
    pub max_interrupters: u16,
    pub max_ports: u8,
    /// Isochronous scheduling threshold, kept for diagnostics only.
    pub isochronous_scheduling_threshold: u8,
    pub max_erst_entries: u16,
    pub max_scratchpad_buffers: u16,
    /// `AC64`: the controller can address memory above 4 GiB.
    pub addressing_64bit: bool,
    /// `CSZ`: contexts are 64 bytes rather than 32.
    pub context_size_64: bool,
    /// `PPC`: port power is software controlled.
    pub port_power_control: bool,
    /// The page size the controller uses, in bytes.
    pub page_size: u32,
    /// Offset of the extended capability list, or 0 if there is none.
    pub extended_capabilities: usize,
    pub hccparams1: u32,
    pub hcsparams1: u32,
    pub hcsparams2: u32,
}

impl Capabilities {
    /// # Safety
    /// `regs` must refer to a live controller.
    pub unsafe fn read(regs: &Regs) -> Self {
        unsafe {
            let hcsparams1 = regs.read_cap_u32(cap::HCSPARAMS1);
            let hcsparams2 = regs.read_cap_u32(cap::HCSPARAMS2);
            let hccparams1 = regs.read_cap_u32(cap::HCCPARAMS1);
            // Max Scratchpad Buffers is split: bits 25..21 are the high five
            // bits and bits 31..27 the low five.
            let scratchpad_hi = (hcsparams2 >> 21) & 0x1f;
            let scratchpad_lo = (hcsparams2 >> 27) & 0x1f;
            let page_size_bits = regs.read_op_u32(op::PAGESIZE) & 0xffff;
            Self {
                hci_version: regs.hci_version(),
                max_slots: (hcsparams1 & 0xff) as u8,
                max_interrupters: ((hcsparams1 >> 8) & 0x7ff) as u16,
                max_ports: ((hcsparams1 >> 24) & 0xff) as u8,
                isochronous_scheduling_threshold: (hcsparams2 & 0xf) as u8,
                max_erst_entries: 1 << ((hcsparams2 >> 4) & 0xf),
                max_scratchpad_buffers: ((scratchpad_hi << 5) | scratchpad_lo) as u16,
                addressing_64bit: hccparams1 & 1 != 0,
                context_size_64: hccparams1 & (1 << 2) != 0,
                port_power_control: hccparams1 & (1 << 3) != 0,
                page_size: if page_size_bits == 0 {
                    4096
                } else {
                    page_size_bits << 12
                },
                extended_capabilities: ((hccparams1 >> 16) as usize) * 4,
                hccparams1,
                hcsparams1,
                hcsparams2,
            }
        }
    }

    #[inline]
    pub const fn context_bytes(&self) -> usize {
        if self.context_size_64 { 64 } else { 32 }
    }
}

/// Extended capability identifiers.
pub mod extended {
    pub const USB_LEGACY_SUPPORT: u8 = 1;
    pub const SUPPORTED_PROTOCOL: u8 = 2;
}

/// One entry of the extended capability list.
#[derive(Clone, Copy, Debug)]
pub struct ExtendedCapability {
    pub id: u8,
    pub offset: usize,
}

/// Walks the extended capability list, calling `f` until it returns `Some`.
///
/// # Safety
/// `regs` must refer to a live controller.
pub unsafe fn find_extended_capability<T>(
    regs: &Regs,
    first: usize,
    mut f: impl FnMut(ExtendedCapability, u32) -> Option<T>,
) -> Option<T> {
    if first == 0 {
        return None;
    }
    let mut offset = first;
    // The list is bounded so a controller with a corrupt `next` pointer cannot
    // keep this spinning.
    for _ in 0..64 {
        let header = unsafe { regs.read_cap_u32(offset) };
        if header == u32::MAX || header == 0 {
            return None;
        }
        let capability = ExtendedCapability {
            id: header as u8,
            offset,
        };
        if let Some(found) = f(capability, header) {
            return Some(found);
        }
        let next = ((header >> 8) & 0xff) as usize * 4;
        if next == 0 {
            return None;
        }
        offset += next;
    }
    None
}

/// The USB major/minor revision a root hub port belongs to, from the
/// Supported Protocol capabilities.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PortProtocol {
    pub major: u8,
    pub minor: u8,
}

impl PortProtocol {
    /// True for a USB 2.0 port, which is the only kind this driver drives.
    #[inline]
    pub const fn is_usb2(&self) -> bool {
        self.major == 2
    }
}

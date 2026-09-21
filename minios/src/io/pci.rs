//! PCI configuration space access, independent of how a host reaches it.
//!
//! Two host bridges matter to `docs/USB_HOST_RPI4_PLAN.md`, and they reach
//! configuration space differently:
//!
//! - **ECAM** (the QEMU virt machine): every function has its own 4 KiB at a
//!   fixed offset in one large window.
//! - **Root port direct, the rest indexed** (the BCM2711 of a Raspberry Pi 4
//!   or 400): the root port's own configuration space sits at the start of the
//!   host's registers, and everything below it is reached by writing the
//!   bus/device/function to an index register and then going through a single
//!   4 KiB data window.
//!
//! [`ConfigAccess`] hides that difference, so bridge numbering, BAR sizing and
//! interrupt routing are written once.  The board-specific numbers — where the
//! index and data registers are, how the link state reads — are parameters
//! supplied by the platform; nothing here knows it is talking to a BCM2711.
//!
//! Register access itself goes through [`RegisterWindow`], so the tricky parts
//! (the root bus aliasing, the link guard) are tested against a fake window on
//! the host rather than only on the board.

/// A bus/device/function triple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bdf {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

impl Bdf {
    pub const fn new(bus: u8, device: u8, function: u8) -> Self {
        Self {
            bus,
            device,
            function,
        }
    }

    /// Bus, device and function packed the way both ECAM and the BCM2711's
    /// index register want them: bus in bits 27..20, device 19..15, function
    /// 14..12.
    #[inline]
    pub const fn config_index(self) -> u32 {
        ((self.bus as u32) << 20) | ((self.device as u32) << 15) | ((self.function as u32) << 12)
    }

    /// The `interrupt-map` child address: bus/device/function in the high
    /// cell, eight bits lower than [`Self::config_index`].
    #[inline]
    pub const fn interrupt_map_address(self) -> u32 {
        self.config_index() >> 4
    }
}

impl core::fmt::Display for Bdf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:02x}:{:02x}.{}", self.bus, self.device, self.function)
    }
}

/// Reads and writes configuration space dwords.
///
/// A function that is not there reads as all ones and ignores writes, which
/// is what real hardware does and what the enumeration code relies on to tell
/// an empty slot from a present device.
pub trait ConfigAccess {
    fn read_u32(&self, bdf: Bdf, offset: u16) -> u32;
    fn write_u32(&self, bdf: Bdf, offset: u16, value: u32);
}

/// A block of registers, addressed by byte offset.
pub trait RegisterWindow {
    fn read(&self, offset: usize) -> u32;
    fn write(&self, offset: usize, value: u32);
    /// Bytes in the window.  Offsets at or past this are never accessed.
    fn len(&self) -> usize;
}

/// A memory-mapped register window.
#[derive(Clone, Copy, Debug)]
pub struct Mmio {
    base: usize,
    len: usize,
}

impl Mmio {
    /// # Safety
    /// `base..base + len` must be mapped device memory that stays mapped for
    /// as long as this value is used, and reading or writing any aligned
    /// dword in it must be harmless in the sense the caller intends.
    pub const unsafe fn new(base: usize, len: usize) -> Self {
        Self { base, len }
    }

    #[inline]
    pub const fn base(&self) -> usize {
        self.base
    }
}

impl RegisterWindow for Mmio {
    #[inline]
    fn read(&self, offset: usize) -> u32 {
        unsafe { ((self.base + offset) as *const u32).read_volatile() }
    }

    #[inline]
    fn write(&self, offset: usize, value: u32) {
        unsafe { ((self.base + offset) as *mut u32).write_volatile(value) }
    }

    #[inline]
    fn len(&self) -> usize {
        self.len
    }
}

/// The dword containing `offset`, or `None` past the 4 KiB of a function.
#[inline]
const fn dword(offset: u16) -> Option<usize> {
    if offset >= 0x1000 {
        None
    } else {
        Some((offset & 0xffc) as usize)
    }
}

/// Enhanced Configuration Access Mechanism: one window, 4 KiB per function.
pub struct Ecam<R: RegisterWindow> {
    regs: R,
    bus_min: u8,
    bus_max: u8,
}

impl<R: RegisterWindow> Ecam<R> {
    /// `regs` starts at bus `bus_min`, which is what the `reg` of a
    /// `pci-host-ecam-generic` node describes.
    pub const fn new(regs: R, bus_min: u8, bus_max: u8) -> Self {
        Self {
            regs,
            bus_min,
            bus_max,
        }
    }

    fn address(&self, bdf: Bdf, offset: u16) -> Option<usize> {
        if bdf.bus < self.bus_min || bdf.bus > self.bus_max || bdf.device > 31 || bdf.function > 7 {
            return None;
        }
        let relative = Bdf::new(bdf.bus - self.bus_min, bdf.device, bdf.function);
        let address = relative.config_index() as usize + dword(offset)?;
        (address + 4 <= self.regs.len()).then_some(address)
    }
}

impl<R: RegisterWindow> ConfigAccess for Ecam<R> {
    fn read_u32(&self, bdf: Bdf, offset: u16) -> u32 {
        match self.address(bdf, offset) {
            Some(address) => self.regs.read(address),
            None => u32::MAX,
        }
    }

    fn write_u32(&self, bdf: Bdf, offset: u16, value: u32) {
        if let Some(address) = self.address(bdf, offset) {
            self.regs.write(address, value)
        }
    }
}

/// Where a host reports whether its link is up: the link counts as up when
/// every bit of `mask` is set in the register at `offset`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinkStatus {
    pub offset: usize,
    pub mask: u32,
}

/// The register layout of a host that exposes its root port directly and
/// everything below it through an index/data pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexedLayout {
    /// Where the root port's own configuration space starts.
    pub root: usize,
    /// The register that selects which function the data window shows.
    pub index: usize,
    /// The start of the 4 KiB window onto the selected function.
    pub data: usize,
    pub link: LinkStatus,
}

/// Root port addressed directly, everything below it through an index
/// register and a data window.
pub struct IndexedConfig<R: RegisterWindow> {
    regs: R,
    root_bus: u8,
    layout: IndexedLayout,
}

impl<R: RegisterWindow> IndexedConfig<R> {
    pub const fn new(regs: R, root_bus: u8, layout: IndexedLayout) -> Self {
        Self {
            regs,
            root_bus,
            layout,
        }
    }

    /// Whether anything below the root port can currently be reached.
    pub fn link_up(&self) -> bool {
        let link = self.layout.link;
        link.offset + 4 <= self.regs.len() && self.regs.read(link.offset) & link.mask == link.mask
    }

    /// Where a read or write of `bdf` at `offset` goes, selecting the function
    /// first if it is below the root port.  `None` means the function is not
    /// reachable and the access must not happen.
    fn route(&self, bdf: Bdf, offset: u16) -> Option<usize> {
        let offset = dword(offset)?;
        if bdf.bus == self.root_bus {
            // Only the root port lives on the root bus.  The window at `root`
            // answers for every device number, so without this check a scan
            // would find thirty-two copies of it.
            if bdf.device != 0 || bdf.function != 0 {
                return None;
            }
            let address = self.layout.root + offset;
            return (address + 4 <= self.regs.len()).then_some(address);
        }
        if bdf.bus < self.root_bus || bdf.device > 31 || bdf.function > 7 {
            return None;
        }
        // A configuration cycle below a link that is not up may never be
        // answered.  Refusing it here is what keeps an early probe from
        // hanging the board.
        if !self.link_up() {
            return None;
        }
        let address = self.layout.data + offset;
        if address + 4 > self.regs.len() || self.layout.index + 4 > self.regs.len() {
            return None;
        }
        self.regs.write(self.layout.index, bdf.config_index());
        Some(address)
    }
}

impl<R: RegisterWindow> ConfigAccess for IndexedConfig<R> {
    fn read_u32(&self, bdf: Bdf, offset: u16) -> u32 {
        match self.route(bdf, offset) {
            Some(address) => self.regs.read(address),
            None => u32::MAX,
        }
    }

    fn write_u32(&self, bdf: Bdf, offset: u16, value: u32) {
        if let Some(address) = self.route(bdf, offset) {
            self.regs.write(address, value)
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;
    use core::cell::RefCell;

    use super::*;

    /// A register window that records every access.
    struct FakeWindow {
        values: RefCell<Vec<u32>>,
        log: RefCell<Vec<(char, usize, u32)>>,
    }

    impl FakeWindow {
        fn new(len: usize) -> Self {
            Self {
                values: RefCell::new(vec![0; len / 4]),
                log: RefCell::new(Vec::new()),
            }
        }

        fn set(&self, offset: usize, value: u32) {
            self.values.borrow_mut()[offset / 4] = value;
        }

        fn writes(&self) -> Vec<(usize, u32)> {
            self.log
                .borrow()
                .iter()
                .filter(|(kind, ..)| *kind == 'w')
                .map(|&(_, offset, value)| (offset, value))
                .collect()
        }

        fn touched(&self) -> bool {
            !self.log.borrow().is_empty()
        }
    }

    impl RegisterWindow for &FakeWindow {
        fn read(&self, offset: usize) -> u32 {
            let value = self.values.borrow()[offset / 4];
            self.log.borrow_mut().push(('r', offset, value));
            value
        }
        fn write(&self, offset: usize, value: u32) {
            self.values.borrow_mut()[offset / 4] = value;
            self.log.borrow_mut().push(('w', offset, value));
        }
        fn len(&self) -> usize {
            self.values.borrow().len() * 4
        }
    }

    /// The BCM2711's numbers, as `platform::arm64dt::rpi::pcie` supplies them.
    const BCM2711: IndexedLayout = IndexedLayout {
        root: 0,
        index: 0x9000,
        data: 0x8000,
        link: LinkStatus {
            offset: 0x4068,
            mask: 0x30,
        },
    };

    #[test]
    fn the_config_index_packs_bus_device_and_function() {
        assert_eq!(Bdf::new(1, 0, 0).config_index(), 1 << 20);
        assert_eq!(
            Bdf::new(1, 2, 3).config_index(),
            (1 << 20) | (2 << 15) | (3 << 12)
        );
        // The interrupt-map child address is the same fields eight bits down.
        assert_eq!(Bdf::new(0, 2, 0).interrupt_map_address(), 0x1000);
    }

    #[test]
    fn ecam_reaches_a_function_at_its_fixed_offset() {
        let window = FakeWindow::new(4 << 20);
        let target = Bdf::new(1, 2, 3).config_index() as usize + 0x10;
        window.set(target, 0xfeed_beef);
        let ecam = Ecam::new(&window, 0, 3);
        assert_eq!(ecam.read_u32(Bdf::new(1, 2, 3), 0x10), 0xfeed_beef);
        // A byte offset reads the dword that contains it.
        assert_eq!(ecam.read_u32(Bdf::new(1, 2, 3), 0x12), 0xfeed_beef);
    }

    #[test]
    fn ecam_outside_its_buses_reads_as_absent_without_touching_anything() {
        let window = FakeWindow::new(4 << 20);
        let ecam = Ecam::new(&window, 0, 3);
        assert_eq!(ecam.read_u32(Bdf::new(4, 0, 0), 0), u32::MAX);
        assert_eq!(ecam.read_u32(Bdf::new(0, 0, 0), 0x1000), u32::MAX);
        ecam.write_u32(Bdf::new(9, 0, 0), 0, 1);
        assert!(!window.touched());
    }

    #[test]
    fn ecam_counts_buses_from_the_start_of_its_window() {
        // A host whose bus-range starts at 2 puts bus 2 at offset zero.
        let window = FakeWindow::new(1 << 20);
        window.set(0, 0x1234_5678);
        let ecam = Ecam::new(&window, 2, 2);
        assert_eq!(ecam.read_u32(Bdf::new(2, 0, 0), 0), 0x1234_5678);
    }

    #[test]
    fn the_root_port_is_read_directly_and_only_once() {
        let window = FakeWindow::new(0x9310);
        window.set(0x0000, 0x2711_14e4);
        let config = IndexedConfig::new(&window, 0, BCM2711);
        assert_eq!(config.read_u32(Bdf::new(0, 0, 0), 0), 0x2711_14e4);
        // The window at `root` would answer for any device number; the other
        // slots on the root bus must read as empty instead.
        assert_eq!(config.read_u32(Bdf::new(0, 1, 0), 0), u32::MAX);
        assert_eq!(config.read_u32(Bdf::new(0, 0, 1), 0), u32::MAX);
        // And the index register is never involved for the root port.
        assert!(window.writes().is_empty());
    }

    #[test]
    fn nothing_below_the_root_port_is_touched_while_the_link_is_down() {
        let window = FakeWindow::new(0x9310);
        // Root complex, but neither the PHY nor the data link is up: exactly
        // what the Raspberry Pi 400 reported with PCIE_PROBE off.
        window.set(0x4068, 0x80);
        let config = IndexedConfig::new(&window, 0, BCM2711);
        assert!(!config.link_up());
        assert_eq!(config.read_u32(Bdf::new(1, 0, 0), 0), u32::MAX);
        config.write_u32(Bdf::new(1, 0, 0), 4, 6);
        assert!(
            window.writes().is_empty(),
            "a cycle below a down link may never be answered"
        );
    }

    #[test]
    fn a_function_below_the_root_port_is_selected_then_read_through_the_window() {
        let window = FakeWindow::new(0x9310);
        window.set(0x4068, 0xb0);
        window.set(0x8000 + 0x08, 0x0c03_3001);
        let config = IndexedConfig::new(&window, 0, BCM2711);
        assert!(config.link_up());
        assert_eq!(config.read_u32(Bdf::new(1, 0, 0), 0x08), 0x0c03_3001);
        assert_eq!(window.writes(), [(0x9000, 1 << 20)]);
    }

    #[test]
    fn a_write_below_the_root_port_selects_first_and_lands_in_the_window() {
        let window = FakeWindow::new(0x9310);
        window.set(0x4068, 0x30);
        let config = IndexedConfig::new(&window, 0, BCM2711);
        config.write_u32(Bdf::new(1, 0, 0), 0x04, 0x0006);
        assert_eq!(
            window.writes(),
            [(0x9000, 1 << 20), (0x8004, 0x0006)],
            "the index has to be written before the data, or the write goes to \
             whichever function was selected last"
        );
    }

    #[test]
    fn offsets_past_the_function_are_refused_on_both_paths() {
        let window = FakeWindow::new(0x9310);
        window.set(0x4068, 0x30);
        let config = IndexedConfig::new(&window, 0, BCM2711);
        assert_eq!(config.read_u32(Bdf::new(0, 0, 0), 0x1000), u32::MAX);
        assert_eq!(config.read_u32(Bdf::new(1, 0, 0), 0x1000), u32::MAX);
        assert!(window.writes().is_empty());
    }
}

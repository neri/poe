//! A report of the Raspberry Pi 4 / 400 PCIe host and what is on it.
//!
//! Stage 1 of `docs/USB_HOST_RPI4_PLAN.md` asks for the real board's state to
//! be recorded before stage 5 touches it: the BCM2711 PCIe host is not the
//! generic ECAM bridge [`crate::platform::arm64dt::pci`] understands, and how
//! much of it the firmware has already set up decides what the driver has to
//! do for itself.  This answers that from the board, over UART.
//!
//! Nothing here configures the link.  Two writes happen, both harmless: if
//! the bridge arrives in software reset, it is released exactly as the first
//! step of Linux's driver does (the `MISC_*` registers do not answer until
//! then); and reaching a device below the root port means writing the
//! bus/device/function to select into `EXT_CFG_INDEX`, which only says what
//! the next read will see.
//!
//! Each step announces itself before it touches the hardware, so a board that
//! stops dead says where it stopped.

use fdt::PropName;

use super::MachineType;
use crate::io::pci::{Bdf, ConfigAccess, IndexedConfig, IndexedLayout, LinkStatus, Mmio};
use crate::platform::arm64dt::pci::PciHost;
use crate::platform::arm64dt::{counter_us, dt};
use crate::usb_println;

/// The host bridges this reports on.
const COMPATIBLE: &[&str] = &["brcm,bcm2711-pcie"];

/// Register offsets, as named by the Linux `pcie-brcmstb` driver.
mod reg {
    /// The root port's own configuration space starts at offset zero.
    pub const RC_CFG_VENDOR_ID: usize = 0x0000;
    pub const MISC_CTRL: usize = 0x4008;
    pub const MISC_CPU_2_PCIE_MEM_WIN0_LO: usize = 0x400c;
    pub const MISC_CPU_2_PCIE_MEM_WIN0_HI: usize = 0x4010;
    pub const MISC_RC_BAR1_CONFIG_LO: usize = 0x402c;
    pub const MISC_RC_BAR2_CONFIG_LO: usize = 0x4034;
    pub const MISC_RC_BAR2_CONFIG_HI: usize = 0x4038;
    pub const MISC_RC_BAR3_CONFIG_LO: usize = 0x403c;
    pub const MISC_MSI_BAR_CONFIG_LO: usize = 0x4044;
    pub const MISC_MSI_DATA_CONFIG: usize = 0x404c;
    pub const MISC_PCIE_CTRL: usize = 0x4064;
    pub const MISC_PCIE_STATUS: usize = 0x4068;
    pub const MISC_REVISION: usize = 0x406c;
    pub const MISC_CPU_2_PCIE_MEM_WIN0_BASE_LIMIT: usize = 0x4070;
    pub const MISC_CPU_2_PCIE_MEM_WIN0_BASE_HI: usize = 0x4080;
    pub const MISC_CPU_2_PCIE_MEM_WIN0_LIMIT_HI: usize = 0x4084;
    pub const MISC_HARD_PCIE_HARD_DEBUG: usize = 0x4204;
    /// Reset generator: bit 1 holds the bridge in software reset, bit 0
    /// asserts PERST# to the endpoint.  It answers while the bridge is in
    /// reset, which the `MISC_*` registers above do not.
    pub const RGR1_SW_INIT_1: usize = 0x9210;
    pub const SW_INIT_BRIDGE: u32 = 1 << 1;
    pub const SW_INIT_PERST: u32 = 1 << 0;
    /// A 4 KiB window onto the configuration space selected by `EXT_CFG_INDEX`.
    pub const EXT_CFG_DATA: usize = 0x8000;
    pub const EXT_CFG_INDEX: usize = 0x9000;

    /// `MISC_PCIE_STATUS` bits.
    pub const STATUS_PORT: u32 = 1 << 7;
    pub const STATUS_DL_ACTIVE: u32 = 1 << 5;
    pub const STATUS_PHYLINKUP: u32 = 1 << 4;
}

/// How the BCM2711 reaches configuration space: the root port at the start
/// of its registers, everything below it through `EXT_CFG_INDEX` and the
/// `EXT_CFG_DATA` window, and only once both the PHY and the data link are up.
pub const CONFIG_LAYOUT: IndexedLayout = IndexedLayout {
    root: reg::RC_CFG_VENDOR_ID,
    index: reg::EXT_CFG_INDEX,
    data: reg::EXT_CFG_DATA,
    link: LinkStatus {
        offset: reg::MISC_PCIE_STATUS,
        mask: reg::STATUS_DL_ACTIVE | reg::STATUS_PHYLINKUP,
    },
};

/// The BCM2711's configuration space accessor over its register window.
pub type Bcm2711Config = IndexedConfig<Mmio>;

/// Finds the BCM2711 host bridge and returns it ready for [`PciHost`]'s
/// bridge numbering and BAR assignment.
///
/// This only builds the host; it does not bring the link up.  Until the link
/// is up, everything below the root port reads as absent, which
/// [`IndexedConfig`] guarantees without touching the hardware.
pub fn host(tree: &fdt::DeviceTree) -> Option<PciHost<Bcm2711Config>> {
    let (node, base, size) = find(tree)?;
    let access = IndexedConfig::new(unsafe { Mmio::new(base, size) }, 0, CONFIG_LAYOUT);
    PciHost::from_node(tree, &node, access)
}

/// The enabled BCM2711 host bridge node and its register window.
fn find<'a>(tree: &'a fdt::DeviceTree) -> Option<(fdt::Node<'a>, usize, usize)> {
    dt::find_map(tree, |node, map| {
        node.is_compatible_with_any(COMPATIBLE)
            .then(|| {
                let (base, size) = map.reg(node, 0)?;
                Some((node.clone(), base, size))
            })
            .flatten()
    })
}

/// Configuration space offsets this reads.
mod config {
    pub const VENDOR_ID: u16 = 0x00;
    pub const COMMAND: u16 = 0x04;
    pub const REVISION_ID: u16 = 0x08;
    pub const HEADER_TYPE: u16 = 0x0e;
    pub const BAR0: u16 = 0x10;
    pub const PRIMARY_BUS: u16 = 0x18;
    pub const MEMORY_BASE: u16 = 0x20;
    pub const INTERRUPT_LINE: u16 = 0x3c;
}

/// Prints what the board looks like right now.  Safe to call on any machine:
/// it does nothing unless the `usb_debug` feature is on and this is a
/// Raspberry Pi 4 or 400 with the expected host bridge in its device tree.
pub fn report(tree: &fdt::DeviceTree) {
    // Only with the diagnostics on.  The prints are silent without
    // `usb_debug`, but the reads are not: a BCM2711 whose PCIe block has been
    // left unclocked stalls every read until the bus gives up, measured at
    // 10.8 s on a Raspberry Pi 400.  A normal boot must not pay that for a
    // report nobody can see.
    if !cfg!(feature = "usb_debug") {
        return;
    }
    if super::current_machine_type() != MachineType::RaspberryPi4 {
        return;
    }
    usb_println!("--- BCM2711 PCIe report ---");

    let Some((node, base, size)) = find(tree) else {
        // `dt::find_map` only visits enabled nodes, so tell the two cases
        // apart: a firmware that switched PCIe off looks nothing like a
        // device tree that never described it.
        if find_disabled(tree) {
            usb_println!(
                "PCIe: the brcm,bcm2711-pcie node is present but disabled. \
                 Something upstream of the kernel turned PCIe off — check the \
                 bootloader's PCIE_PROBE setting and any overlay in config.txt."
            );
        } else {
            usb_println!("PCIe: no brcm,bcm2711-pcie node in the device tree");
        }
        return;
    };
    usb_println!("PCIe: registers at {:#x}, {:#x} bytes", base, size);
    report_device_tree(tree, &node);

    if size < reg::EXT_CFG_INDEX + 4 {
        usb_println!("PCIe: the register window is too small to reach config space");
        return;
    }

    // From here on the report touches the hardware.  If the firmware left the
    // block unpowered or in reset this is where a board would stop, so say so
    // before the first access rather than after it.
    // The reset generator first.  Linux's BCM2711 driver never touches the
    // MISC registers until it has taken the bridge out of software reset
    // through this one register, and with the current bootloader the bridge
    // arrives in reset: reading MISC then stalls until the bus gives up
    // (10.8 s) and returns zero.
    usb_println!(
        "PCIe: counter at {} ms; reading the reset generator...",
        counter_us() / 1000
    );
    let (sw_init, elapsed) = timed_read(base + reg::RGR1_SW_INIT_1);
    usb_println!(
        "PCIe: RGR1_SW_INIT_1={:08x} ({} us) bridge {}, PERST# {}",
        sw_init,
        elapsed,
        if sw_init & reg::SW_INIT_BRIDGE != 0 {
            "in software reset"
        } else {
            "running"
        },
        if sw_init & reg::SW_INIT_PERST != 0 {
            "asserted"
        } else {
            "deasserted"
        },
    );
    if sw_init == u32::MAX || elapsed > 1_000_000 {
        usb_println!(
            "PCIe: the reset generator is not answering either, so the whole block is \
             unreachable.  Nothing further is read."
        );
        usb_println!("--- end of PCIe report ---");
        return;
    }
    if sw_init & reg::SW_INIT_BRIDGE != 0 {
        // The one deliberate write of this report, and exactly the first thing
        // Linux does in `brcm_pcie_probe`: clear the bridge reset and leave
        // every other bit — PERST# included — as it was.  A power cycle puts
        // it back.
        usb_println!("PCIe: releasing the bridge from software reset, as Linux does first");
        unsafe {
            ((base + reg::RGR1_SW_INIT_1) as *mut u32)
                .write_volatile(sw_init & !reg::SW_INIT_BRIDGE)
        };
        let released = counter_us();
        while counter_us().wrapping_sub(released) < 200 {
            core::hint::spin_loop();
        }
    }

    usb_println!("PCIe: reading controller registers...");
    let (revision, elapsed) = timed_read(base + reg::MISC_REVISION);
    usb_println!(
        "PCIe: REVISION={:08x} ({} us for this read)",
        revision,
        elapsed
    );
    if revision == 0 || revision == u32::MAX {
        usb_println!(
            "PCIe: the controller is still not answering: its fixed revision register \
             reads {:08x}.  Nothing further is read.",
            revision
        );
        usb_println!("--- end of PCIe report ---");
        return;
    }
    let status = unsafe { read32(base + reg::MISC_PCIE_STATUS) };
    report_controller(base, status);

    let link_up = status & (reg::STATUS_DL_ACTIVE | reg::STATUS_PHYLINKUP)
        == (reg::STATUS_DL_ACTIVE | reg::STATUS_PHYLINKUP);
    if !link_up {
        usb_println!(
            "PCIe: link is down, so nothing below the root port is readable.  \
             Bringing it up (PERST# release, windows, link training) is stage 5."
        );
        usb_println!("--- end of PCIe report ---");
        return;
    }

    // Configuration space goes through the same accessor the driver will
    // use, so this report is also its first test on real hardware.
    let access = IndexedConfig::new(unsafe { Mmio::new(base, size) }, 0, CONFIG_LAYOUT);
    usb_println!("PCIe: reading the root port's configuration space...");
    let secondary = report_root_port(&access);

    match secondary {
        0 => usb_println!("PCIe: the root port has no secondary bus number assigned yet"),
        bus => {
            usb_println!("PCIe: reading bus {} through EXT_CFG_INDEX...", bus);
            report_downstream(&access, bus);
        }
    }
    usb_println!("--- end of PCIe report ---");
}

/// Looks for the host bridge without the enabled-only filter, to at least a
/// bus deep — on a Raspberry Pi 4 it lives at `/scb/pcie@7d500000`.
fn find_disabled(tree: &fdt::DeviceTree) -> bool {
    fn matches(node: &fdt::Node) -> bool {
        node.is_compatible_with_any(COMPATIBLE)
    }
    tree.root()
        .children()
        .any(|child| matches(&child) || child.children().any(|grandchild| matches(&grandchild)))
}

/// What the device tree says, which is what the driver will have to act on.
fn report_device_tree(tree: &fdt::DeviceTree, node: &fdt::Node) {
    if let Some(words) = node.get_prop(PropName::INTERRUPTS).map(|v| v.words()) {
        // A GIC specifier is <type number flags>; type 0 is an SPI, whose
        // interrupt number is 32 higher than the number given here.
        for (index, entry) in words.chunks_exact(3).enumerate() {
            let kind = entry[0].as_u32();
            let number = entry[1].as_u32();
            usb_println!(
                "PCIe: interrupts[{}] = {} {} {} -> IRQ {}",
                index,
                kind,
                number,
                entry[2].as_u32(),
                if kind == 0 { number + 32 } else { number + 16 }
            );
        }
    }
    // `ranges` and `dma-ranges` of a PCI host are
    // <pci-addr(3) parent-addr(2) size(2)>.
    for (name, prop) in [
        ("ranges", PropName::RANGES),
        ("dma-ranges", PropName::DMA_RANGES),
    ] {
        let Some(words) = node.get_prop(prop).map(|v| v.words()) else {
            usb_println!("PCIe: {} absent", name);
            continue;
        };
        for entry in words.chunks_exact(7) {
            let flags = entry[0].as_u32();
            let pci = ((entry[1].as_u32() as u64) << 32) | entry[2].as_u32() as u64;
            let cpu = ((entry[3].as_u32() as u64) << 32) | entry[4].as_u32() as u64;
            let len = ((entry[5].as_u32() as u64) << 32) | entry[6].as_u32() as u64;
            usb_println!(
                "PCIe: {} flags {:08x} pci {:#x} cpu {:#x} len {:#x}",
                name,
                flags,
                pci,
                cpu,
                len
            );
        }
    }
    if let Some(words) = node
        .get_prop(PropName::INTERRUPT_MAP)
        .map(|v| v.words())
        .filter(|words| words.len() >= 8)
    {
        usb_println!("PCIe: interrupt-map has {} words", words.len());
    }
    report_dependencies(tree, node);
}

/// What the node says has to be on before the block works.
///
/// Linux reaches this block with it already answering, and nothing in its
/// PCIe driver's log says it turned anything on.  If an earlier driver did,
/// the node is where the dependency would be declared: a clock, a reset or a
/// power domain, each a phandle to a provider plus that provider's arguments.
fn report_dependencies(tree: &fdt::DeviceTree, node: &fdt::Node) {
    for (name, cells) in [
        ("clocks", "#clock-cells"),
        ("resets", "#reset-cells"),
        ("power-domains", "#power-domain-cells"),
    ] {
        let Some(words) = node.get_prop(PropName(name)).map(|v| v.words()) else {
            usb_println!("PCIe: {} absent", name);
            continue;
        };
        let mut index = 0;
        while index < words.len() {
            let phandle = fdt::PHandle(words[index].as_u32());
            index += 1;
            let Some(provider) = tree.find_by_phandle(phandle) else {
                usb_println!("PCIe: {} -> unknown phandle {:#x}", name, phandle.0);
                break;
            };
            let count = provider.get_prop_u32(PropName(cells)).unwrap_or(0) as usize;
            let args = &words[index..(index + count).min(words.len())];
            index += count;
            let mut text = heapless::String::<48>::new();
            for arg in args {
                let _ = core::fmt::Write::write_fmt(&mut text, format_args!(" {}", arg.as_u32()));
            }
            usb_println!(
                "PCIe: {} -> {} ({}){}",
                name,
                provider.name(),
                provider
                    .get_prop_str(PropName::COMPATIBLE)
                    .unwrap_or("no compatible"),
                text
            );
        }
    }
    for name in ["clock-names", "reset-names", "power-domain-names"] {
        if let Some(prop) = node.get_prop(PropName(name)) {
            for (index, entry) in prop.string_list().enumerate() {
                usb_println!("PCIe: {}[{}] = {}", name, index, entry);
            }
        }
    }
}

/// The controller's own state, as the firmware left it.
fn report_controller(base: usize, status: u32) {
    unsafe {
        usb_println!(
            "PCIe: MISC_CTRL={:08x} PCIE_CTRL={:08x} HARD_DEBUG={:08x}",
            read32(base + reg::MISC_CTRL),
            read32(base + reg::MISC_PCIE_CTRL),
            read32(base + reg::MISC_HARD_PCIE_HARD_DEBUG),
        );
        usb_println!(
            "PCIe: STATUS={:08x} ({}, phy link {}, data link {})",
            status,
            if status & reg::STATUS_PORT != 0 {
                "root complex"
            } else {
                "endpoint"
            },
            if status & reg::STATUS_PHYLINKUP != 0 {
                "up"
            } else {
                "down"
            },
            if status & reg::STATUS_DL_ACTIVE != 0 {
                "active"
            } else {
                "inactive"
            },
        );
        // The outbound window is what maps CPU addresses onto the bus, and
        // the inbound BAR2 is what limits where a device may do DMA.
        usb_println!(
            "PCIe: out win0 lo={:08x} hi={:08x} base_limit={:08x} base_hi={:08x} limit_hi={:08x}",
            read32(base + reg::MISC_CPU_2_PCIE_MEM_WIN0_LO),
            read32(base + reg::MISC_CPU_2_PCIE_MEM_WIN0_HI),
            read32(base + reg::MISC_CPU_2_PCIE_MEM_WIN0_BASE_LIMIT),
            read32(base + reg::MISC_CPU_2_PCIE_MEM_WIN0_BASE_HI),
            read32(base + reg::MISC_CPU_2_PCIE_MEM_WIN0_LIMIT_HI),
        );
        usb_println!(
            "PCIe: in BAR1={:08x} BAR2={:08x}/{:08x} BAR3={:08x} MSI={:08x}/{:08x}",
            read32(base + reg::MISC_RC_BAR1_CONFIG_LO),
            read32(base + reg::MISC_RC_BAR2_CONFIG_LO),
            read32(base + reg::MISC_RC_BAR2_CONFIG_HI),
            read32(base + reg::MISC_RC_BAR3_CONFIG_LO),
            read32(base + reg::MISC_MSI_BAR_CONFIG_LO),
            read32(base + reg::MISC_MSI_DATA_CONFIG),
        );
    }
}

/// The root port's configuration space.  Returns the secondary bus number it
/// has been given, or zero if it has none.
fn report_root_port(access: &impl ConfigAccess) -> u8 {
    let root = Bdf::new(0, 0, 0);
    let read = |offset| access.read_u32(root, offset);
    let ids = read(config::VENDOR_ID);
    let class = read(config::REVISION_ID);
    let command = read(config::COMMAND);
    let buses = read(config::PRIMARY_BUS);
    let memory = read(config::MEMORY_BASE);
    let header = read(config::HEADER_TYPE);
    usb_println!(
        "PCIe RC: {:04x}:{:04x} class {:02x}/{:02x}/{:02x} header {:02x} COMMAND={:04x}",
        ids & 0xffff,
        ids >> 16,
        (class >> 24) & 0xff,
        (class >> 16) & 0xff,
        (class >> 8) & 0xff,
        (header >> 16) & 0xff,
        command & 0xffff,
    );
    usb_println!(
        "PCIe RC: primary {} secondary {} subordinate {} mem window {:04x}-{:04x}",
        buses & 0xff,
        (buses >> 8) & 0xff,
        (buses >> 16) & 0xff,
        memory & 0xffff,
        memory >> 16,
    );
    ((buses >> 8) & 0xff) as u8
}

/// Whatever is on `bus`, which on a Raspberry Pi 4 or 400 should be the VL805.
fn report_downstream(access: &impl ConfigAccess, bus: u8) {
    for device in 0..2u8 {
        let bdf = Bdf::new(bus, device, 0);
        let read = |offset| access.read_u32(bdf, offset);
        let ids = read(config::VENDOR_ID);
        if ids == u32::MAX || ids & 0xffff == 0 {
            continue;
        }
        let class = read(config::REVISION_ID);
        let command = read(config::COMMAND);
        let bar0 = read(config::BAR0);
        let bar1 = read(config::BAR0 + 4);
        let interrupt = read(config::INTERRUPT_LINE);
        usb_println!(
            "PCIe {}: {:04x}:{:04x} rev {:02x} class {:02x}/{:02x}/{:02x}",
            bdf,
            ids & 0xffff,
            ids >> 16,
            class & 0xff,
            (class >> 24) & 0xff,
            (class >> 16) & 0xff,
            (class >> 8) & 0xff,
        );
        usb_println!(
            "PCIe {}: COMMAND={:04x} BAR0={:08x} BAR1={:08x} pin {} line {}",
            bdf,
            command & 0xffff,
            bar0,
            bar1,
            (interrupt >> 8) & 0xff,
            interrupt & 0xff,
        );
        // 0c/03/30 is an xHCI controller.  Saying so plainly is the whole
        // point of the exercise: it is what stage 5 has to drive.
        if (class >> 8) & 0xff_ffff == 0x0c_0330 {
            usb_println!(
                "PCIe {}: this is the xHCI controller{}",
                bdf,
                if command & 0x6 == 0x6 {
                    ", already decoding memory with bus mastering on"
                } else {
                    ", not yet enabled by the firmware"
                }
            );
        }
    }
}

/// One read, and how long it took in microseconds.
fn timed_read(address: usize) -> (u32, u64) {
    let started = counter_us();
    let value = unsafe { read32(address) };
    (value, counter_us().wrapping_sub(started))
}

#[inline]
unsafe fn read32(address: usize) -> u32 {
    unsafe { (address as *const u32).read_volatile() }
}

// ---- Stage 5: bringing the link up ----------------------------------------
//
// A port of `brcm_pcie_setup` and `brcm_pcie_start_link` from the Raspberry
// Pi Linux tree (rpi-6.18.y, `drivers/pci/controller/pcie-brcmstb.c`),
// restricted to the BCM2711 path: `bcm2711_cfg`, no `bridge`/`swinit`
// resets, no clock, no rescal PHY.  Register and field names follow that
// file so the two can be read side by side.

/// Bring-up registers and fields that the report does not use.
mod setup {
    pub const RC_CFG_VENDOR_SPECIFIC_REG1: usize = 0x0188;
    pub const RC_CFG_VENDOR_SPECIFIC_REG1_ENDIAN_MODE_BAR2: u32 = 0xc;
    pub const RC_CFG_PRIV1_ID_VAL3: usize = 0x043c;
    pub const RC_CFG_PRIV1_ID_VAL3_CLASS_CODE: u32 = 0x00ff_ffff;
    /// The root port's PCI Express capability, where link status lives.
    pub const PCIE_CAP_REGS: usize = 0x00ac;
    pub const PCI_EXP_LNKSTA: usize = 0x12;

    pub const MISC_CTRL_PCIE_RCB_64B_MODE: u32 = 0x80;
    pub const MISC_CTRL_PCIE_RCB_MPS_MODE: u32 = 0x400;
    pub const MISC_CTRL_SCB_ACCESS_EN: u32 = 0x1000;
    pub const MISC_CTRL_CFG_READ_UR_MODE: u32 = 0x2000;
    pub const MISC_CTRL_MAX_BURST_SIZE: u32 = 0x0030_0000;
    pub const MISC_CTRL_SCB0_SIZE: u32 = 0xf800_0000;

    pub const RC_BAR1_CONFIG_LO: usize = 0x402c;
    pub const RC_BAR_CONFIG_LO_SIZE: u32 = 0x1f;

    pub const MEM_WIN0_BASE_LIMIT_LIMIT: u32 = 0xfff0_0000;
    pub const MEM_WIN0_BASE_LIMIT_BASE: u32 = 0x0000_fff0;
    pub const MEM_WIN0_HI_MASK: u32 = 0xff;

    pub const HARD_DEBUG_CLKREQ_DEBUG_ENABLE: u32 = 0x2;
    pub const HARD_DEBUG_REFCLK_OVRD_ENABLE: u32 = 0x0001_0000;
    pub const HARD_DEBUG_REFCLK_OVRD_OUT: u32 = 0x0010_0000;
    pub const HARD_DEBUG_L1SS_ENABLE: u32 = 0x0020_0000;
    pub const HARD_DEBUG_SERDES_IDDQ: u32 = 0x0800_0000;
    pub const CLKREQ_MASK: u32 = HARD_DEBUG_CLKREQ_DEBUG_ENABLE
        | HARD_DEBUG_REFCLK_OVRD_ENABLE
        | HARD_DEBUG_REFCLK_OVRD_OUT
        | HARD_DEBUG_L1SS_ENABLE;

    /// Two registers before `RGR1_SW_INIT_1`: how long an internal bus access
    /// may take, in 1/216 MHz units.  Its reset value is what made every read
    /// of a bridge in reset take 10.8 s.
    pub const RBUS_TIMEOUT: usize = super::reg::RGR1_SW_INIT_1 - 8;

    /// The VL805 is hard-wired at 01:00.0 on every BCM2711 board.
    pub const VL805_ADDRESS: u32 = 0x10_0000;
}

/// The link as it came up.
#[derive(Clone, Copy, Debug)]
pub struct LinkInfo {
    /// 1 = 2.5 GT/s, 2 = 5.0 GT/s.
    pub speed: u8,
    pub width: u8,
    /// Microseconds from releasing PERST# to the link reporting up.
    pub training_us: u64,
}

/// `brcm_pcie_encode_ibar_size`: the size field of an inbound window.
const fn encode_ibar_size(size: u64) -> u32 {
    let log2 = 63 - size.leading_zeros();
    if log2 >= 12 && log2 <= 15 {
        (log2 - 12) + 0x1c
    } else if log2 >= 16 && log2 <= 36 {
        log2 - 15
    } else {
        0
    }
}

/// `1 << fls64(size - 1)`: the power of two at or above `size`.
const fn round_up_pow2(size: u64) -> u64 {
    if size <= 1 {
        1
    } else {
        1 << (64 - (size - 1).leading_zeros())
    }
}

// Checked at build time, because this path cannot run under QEMU.  The
// expected values are Linux's encodings applied to the Raspberry Pi 400's
// actual windows (outbound PCI 0xc000_0000 / CPU 0x6_0000_0000 / 1 GiB,
// inbound PCI 0x4_0000_0000 / CPU 0 / 4 GiB).
const _: () = {
    // Inbound size field: 4 KiB..32 KiB are 0x1c..0x1f, 64 KiB.. are log2-15.
    assert!(encode_ibar_size(1 << 12) == 0x1c);
    assert!(encode_ibar_size(1 << 15) == 0x1f);
    assert!(encode_ibar_size(1 << 16) == 1);
    assert!(encode_ibar_size(1 << 32) == 17);
    assert!(encode_ibar_size(1 << 37) == 0);
    // A 3 GiB board still gets a 4 GiB viewport.
    assert!(round_up_pow2(3 << 30) == 1 << 32);
    assert!(round_up_pow2(1 << 32) == 1 << 32);
    // SCB0_SIZE for a 4 GiB port.
    assert!(field(setup::MISC_CTRL_SCB0_SIZE, 17) == 17 << 27);
    // Outbound window in MB: base 0x6000, limit 0x63ff; the low twelve bits go
    // into BASE_LIMIT and the rest into BASE_HI / LIMIT_HI.
    let cpu_mb = 0x6_0000_0000u64 >> 20;
    let limit_mb = (0x6_0000_0000u64 + 0x4000_0000 - 1) >> 20;
    assert!(cpu_mb == 0x6000 && limit_mb == 0x63ff);
    assert!(field(setup::MEM_WIN0_BASE_LIMIT_BASE, cpu_mb as u32) == 0);
    assert!(field(setup::MEM_WIN0_BASE_LIMIT_LIMIT, limit_mb as u32) == 0x3ff0_0000);
    let shift = setup::MEM_WIN0_BASE_LIMIT_BASE.count_ones();
    assert!(shift == 12 && cpu_mb >> shift == 6 && limit_mb >> shift == 6);
    // The RBUS timeout register sits two registers below RGR1_SW_INIT_1.
    assert!(setup::RBUS_TIMEOUT == 0x9208);
};

#[inline]
fn delay_us(us: u64) {
    let start = counter_us();
    while counter_us().wrapping_sub(start) < us {
        core::hint::spin_loop();
    }
}

struct Regs(usize);

impl Regs {
    #[inline]
    fn read(&self, offset: usize) -> u32 {
        unsafe { read32(self.0 + offset) }
    }
    #[inline]
    fn write(&self, offset: usize, value: u32) {
        unsafe { ((self.0 + offset) as *mut u32).write_volatile(value) }
    }
    /// Read-modify-write: clear `mask`, then set `value` (already shifted).
    #[inline]
    fn update(&self, offset: usize, mask: u32, value: u32) {
        let old = self.read(offset);
        self.write(offset, (old & !mask) | (value & mask));
    }
    #[inline]
    fn read16(&self, offset: usize) -> u16 {
        unsafe { ((self.0 + offset) as *const u16).read_volatile() }
    }
}

/// `u32p_replace_bits`: put `value` into the field `mask` describes.
const fn field(mask: u32, value: u32) -> u32 {
    (value << mask.trailing_zeros()) & mask
}

/// Resets the bridge, programs its windows and trains the link, as Linux's
/// driver does on this SoC.  Leaves the root port ready for configuration
/// cycles to bus 1.
///
/// `outbound` is `(pci, cpu, len)` of the host's memory window and `dma` its
/// inbound view of RAM, both from the device tree.
///
/// # Safety
/// `base` must be the BCM2711 PCIe register window, and nothing else may be
/// using the controller.
pub unsafe fn bring_up_link(
    base: usize,
    outbound: (u64, u64, u64),
    dma: crate::platform::arm64dt::pci::DmaWindow,
) -> Result<LinkInfo, &'static str> {
    let r = Regs(base);

    // --- brcm_pcie_setup ---------------------------------------------------
    // Reset the bridge and make sure PERST# is asserted; some bootloaders
    // deassert it.  Both live in RGR1_SW_INIT_1 on this SoC.
    r.update(
        reg::RGR1_SW_INIT_1,
        reg::SW_INIT_BRIDGE | reg::SW_INIT_PERST,
        reg::SW_INIT_BRIDGE | reg::SW_INIT_PERST,
    );
    delay_us(200);
    // Take the bridge out of reset.  PERST# stays asserted until the end.
    r.update(reg::RGR1_SW_INIT_1, reg::SW_INIT_BRIDGE, 0);

    // Power the SerDes up, and give it time to settle.
    r.update(
        reg::MISC_HARD_PCIE_HARD_DEBUG,
        setup::HARD_DEBUG_SERDES_IDDQ,
        0,
    );
    delay_us(200);

    // SCB access, config reads of an absent function return all ones rather
    // than aborting, 128-byte bursts (the BCM2711 encoding of 0), and the
    // read completion boundary modes.
    let mut misc = r.read(reg::MISC_CTRL);
    misc |= setup::MISC_CTRL_SCB_ACCESS_EN
        | setup::MISC_CTRL_CFG_READ_UR_MODE
        | setup::MISC_CTRL_PCIE_RCB_MPS_MODE
        | setup::MISC_CTRL_PCIE_RCB_64B_MODE;
    misc &= !setup::MISC_CTRL_MAX_BURST_SIZE;
    r.write(reg::MISC_CTRL, misc);

    // Inbound windows.  BAR1 and BAR3 are disabled; BAR2 is the whole of
    // RAM, a power of two in size and aligned to it on the PCI side.
    let dma_len = dma.cpu_end.saturating_sub(dma.cpu_start);
    let inbound_size = round_up_pow2(dma_len);
    let inbound_pci = dma.cpu_start.wrapping_add(dma.offset);
    if dma_len == 0 || inbound_pci & (inbound_size - 1) != 0 {
        return Err("dma-ranges does not describe a usable inbound window");
    }
    for (bar, pci, size) in [
        (1usize, 0u64, 0u64),
        (2, inbound_pci, inbound_size),
        (3, 0, 0),
    ] {
        let offset = setup::RC_BAR1_CONFIG_LO + 8 * (bar - 1);
        let lo = (pci as u32 & !setup::RC_BAR_CONFIG_LO_SIZE)
            | if size == 0 { 0 } else { encode_ibar_size(size) };
        r.write(offset, lo);
        r.write(offset + 4, (pci >> 32) as u32);
    }

    if r.read(reg::MISC_PCIE_STATUS) & reg::STATUS_PORT == 0 {
        return Err("the controller is strapped as an endpoint, not a root complex");
    }

    // One memory controller, seen through a port the size of the inbound
    // window: SCB0_SIZE is log2 of it, less 15.
    let scb_size = (63 - inbound_size.leading_zeros()).saturating_sub(15);
    r.update(
        reg::MISC_CTRL,
        setup::MISC_CTRL_SCB0_SIZE,
        field(setup::MISC_CTRL_SCB0_SIZE, scb_size),
    );

    // Present the root port as a PCI-to-PCI bridge; it resets as an endpoint.
    r.update(
        setup::RC_CFG_PRIV1_ID_VAL3,
        setup::RC_CFG_PRIV1_ID_VAL3_CLASS_CODE,
        0x06_0400,
    );

    // Outbound window 0: the host's memory window, CPU address to PCI.
    let (pci, cpu, len) = outbound;
    r.write(reg::MISC_CPU_2_PCIE_MEM_WIN0_LO, pci as u32);
    r.write(reg::MISC_CPU_2_PCIE_MEM_WIN0_HI, (pci >> 32) as u32);
    let cpu_mb = cpu / (1 << 20);
    let limit_mb = (cpu + len - 1) / (1 << 20);
    let high_shift = setup::MEM_WIN0_BASE_LIMIT_BASE.count_ones();
    r.update(
        reg::MISC_CPU_2_PCIE_MEM_WIN0_BASE_LIMIT,
        setup::MEM_WIN0_BASE_LIMIT_BASE | setup::MEM_WIN0_BASE_LIMIT_LIMIT,
        field(setup::MEM_WIN0_BASE_LIMIT_BASE, cpu_mb as u32)
            | field(setup::MEM_WIN0_BASE_LIMIT_LIMIT, limit_mb as u32),
    );
    r.update(
        reg::MISC_CPU_2_PCIE_MEM_WIN0_BASE_HI,
        setup::MEM_WIN0_HI_MASK,
        (cpu_mb >> high_shift) as u32,
    );
    r.update(
        reg::MISC_CPU_2_PCIE_MEM_WIN0_LIMIT_HI,
        setup::MEM_WIN0_HI_MASK,
        (limit_mb >> high_shift) as u32,
    );

    // Inbound data through BAR2 is little endian.
    r.update(
        setup::RC_CFG_VENDOR_SPECIFIC_REG1,
        setup::RC_CFG_VENDOR_SPECIFIC_REG1_ENDIAN_MODE_BAR2,
        0,
    );

    // --- brcm_pcie_start_link ----------------------------------------------
    // CLKREQ# input off before link-up, then release PERST#.
    r.update(reg::MISC_HARD_PCIE_HARD_DEBUG, setup::CLKREQ_MASK, 0);
    r.update(reg::RGR1_SW_INIT_1, reg::SW_INIT_PERST, 0);
    let released = counter_us();

    // 100 ms after PERST# per the CEM specification, then up to another
    // 100 ms of polling for the link.
    delay_us(100_000);
    let link_bits = reg::STATUS_DL_ACTIVE | reg::STATUS_PHYLINKUP;
    let mut waited = 0;
    while r.read(reg::MISC_PCIE_STATUS) & link_bits != link_bits && waited < 100_000 {
        delay_us(5_000);
        waited += 5_000;
    }
    if r.read(reg::MISC_PCIE_STATUS) & link_bits != link_bits {
        return Err("the link did not come up within 200 ms of releasing PERST#");
    }
    let training_us = counter_us().wrapping_sub(released);

    // CLKREQ# "default" mode, the one Linux reports on this board: L1
    // substates allowed, with the internal bus timeout extended to 4 s.
    r.update(
        reg::MISC_HARD_PCIE_HARD_DEBUG,
        setup::CLKREQ_MASK,
        setup::HARD_DEBUG_L1SS_ENABLE,
    );
    r.write(setup::RBUS_TIMEOUT, 216 * 4_000_000);

    let lnksta = r.read16(setup::PCIE_CAP_REGS + setup::PCI_EXP_LNKSTA);
    Ok(LinkInfo {
        speed: (lnksta & 0xf) as u8,
        width: ((lnksta >> 4) & 0x3f) as u8,
        training_us,
    })
}

/// Brings the BCM2711's PCIe link up, has the firmware load the VL805, and
/// starts the xHCI stack on it — the Raspberry Pi 4 / 400 counterpart of
/// `xhci_pci::init` on the QEMU virt machine.
///
/// # Safety
/// Nothing else may be using the PCIe controller.
pub unsafe fn init_usb(tree: &fdt::DeviceTree) -> Result<(), &'static str> {
    use crate::platform::arm64dt::xhci_pci;

    let (_, base, size) = find(tree).ok_or("no brcm,bcm2711-pcie node in the device tree")?;
    if size < reg::EXT_CFG_INDEX + 4 {
        return Err("the PCIe register window is too small");
    }
    let mut host = host(tree).ok_or("the PCIe node describes no memory window")?;
    let outbound = host
        .mem32_window()
        .ok_or("the PCIe node has no 32-bit memory window")?;
    let dma = host.dma_window();

    let link = unsafe { bring_up_link(base, outbound, dma)? };
    usb_println!(
        "PCIe: link up, {} GT/s x{}, {} ms after PERST#",
        match link.speed {
            1 => "2.5",
            2 => "5.0",
            _ => "?",
        },
        link.width,
        link.training_us / 1000
    );

    // Number the root port so configuration cycles reach bus 1, then make
    // sure the VL805 is there before asking the firmware to load it.
    host.configure_bridges();
    let vl805 = Bdf::new(1, 0, 0);
    let ids = host.read_u32(vl805, 0);
    usb_println!(
        "PCIe {}: {:04x}:{:04x} before firmware load",
        vl805,
        ids & 0xffff,
        ids >> 16
    );
    if ids == u32::MAX || ids & 0xffff == 0 {
        return Err("no device at 01:00.0 after link-up");
    }

    // The VL805 lost its firmware with PERST#.  The VideoCore holds both the
    // blob and the loader; Linux's reset-raspberrypi asks for it this way
    // and then waits up to a millisecond.
    super::mbox::notify_xhci_reset(setup::VL805_ADDRESS)
        .map_err(|_| "the firmware refused to load the VL805")?;
    delay_us(1_000);
    usb_println!("PCIe: VL805 firmware load requested");

    unsafe { xhci_pci::init_with(&mut host) }
}

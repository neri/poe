//! VisionFive 2 PCIe root-port bring-up and VL805 discovery.
//!
//! The attached vendor DT has two enabled PLDA root ports. It does not say
//! which one has the VL805 on this particular board, and it may differ from
//! the DTB U-Boot actually passes. Only initialize the exact vendor DT layout
//! whose clock and reset IDs have been checked against the board boot log.

use core::sync::atomic::{AtomicUsize, Ordering};

use fdt::{DeviceTree, Node, PHandle, PropName};

use super::super::timer::PlatformTimer;
use crate::{System, println};

// Set only after the linked VL805's BAR and xHCI capability are verified.
static VL805_ENDPOINT: AtomicUsize = AtomicUsize::new(0);

pub(super) fn report(tree: &DeviceTree) {
    let mut found = Inventory::default();
    walk(tree.root(), 0, &mut found);
    if found.pcie_enabled == 0 || found.hdmi_enabled == 0 {
        println!(
            "JH7110: boot DTB disables a required controller (PCIe {}/{}, HDMI {}/{} enabled)",
            found.pcie_enabled, found.pcie_total, found.hdmi_enabled, found.hdmi_total,
        );
    }
    probe_links(tree);
}

/// Linux's JH7110 PCIe driver reads bit 5 at the final offset of
/// `starfive,stg-syscon` to determine whether the data link is active. This
/// snapshot is read-only and does not assume U-Boot left an ATU programmed.
fn probe_links(tree: &DeviceTree) {
    let Some(soc) = tree.root().find_first_child(fdt::NodeName("soc")) else {
        return;
    };
    for port in soc.children() {
        if !port.status_is_ok() || !port.is_compatible_with("starfive,jh7110-pcie") {
            continue;
        }
        let Some(prop) = port.get_prop(PropName("starfive,stg-syscon")) else {
            println!("JH7110 PCIe {}: no STG syscon link descriptor", port.name());
            continue;
        };
        let [phandle, _, _, _, link, ..] = prop.words() else {
            println!("JH7110 PCIe {}: invalid STG syscon descriptor", port.name());
            continue;
        };
        let Some(syscon) = tree.find_by_phandle(PHandle(phandle.as_u32())) else {
            println!("JH7110 PCIe {}: STG syscon not found", port.name());
            continue;
        };
        let Some((base, size)) = syscon.reg().and_then(|mut regs| regs.next()) else {
            println!("JH7110 PCIe {}: STG syscon has no registers", port.name());
            continue;
        };
        let offset = link.as_u32() as u64;
        if offset % 4 != 0 || offset.checked_add(4).is_none_or(|end| end > size) {
            println!(
                "JH7110 PCIe {}: link offset outside STG syscon",
                port.name()
            );
            continue;
        }
        let Some(address) = base
            .checked_add(offset)
            .and_then(|a| usize::try_from(a).ok())
        else {
            println!("JH7110 PCIe {}: invalid link register address", port.name());
            continue;
        };
        let value = unsafe { (address as *const u32).read_volatile() };
        // These offsets are the PCIe control/status registers in the STG
        // syscon. Reading them does not require the PLDA controller clock.
        let port_base = if offset == 0x1b8 {
            0x48
        } else if offset == 0x368 {
            0x1f8
        } else {
            continue;
        };
        if value & (1 << 5) == 0 {
            init_port(tree, &port, base, size, port_base, address);
        } else {
            // Only a cold root port is supported; firmware's ATR and BAR
            // layout is not reused.
            println!(
                "JH7110 PCIe {}: link already active (initialized by firmware), skipping setup",
                port.name()
            );
        }
    }
}

fn update_bits(address: usize, mask: u32, bits: u32) -> u32 {
    let register = address as *mut u32;
    let old = unsafe { register.read_volatile() };
    let new = (old & !mask) | (bits & mask);
    unsafe { register.write_volatile(new) };
    new
}

fn update_pci_command(address: usize, mask: u16, bits: u16) -> u16 {
    let register = address as *mut u16;
    let old = unsafe { register.read_volatile() };
    let new = (old & !mask) | (bits & mask);
    unsafe { register.write_volatile(new) };
    new
}

/// Initialize only the two ports described by the supplied vendor DTB.
/// PLDA address windows are still separate work; this step makes the root
/// ports accessible and performs the board's PERST pulse.
fn init_port(
    tree: &DeviceTree,
    port: &Node,
    stg_base: u64,
    stg_size: u64,
    port_base: u64,
    link: usize,
) {
    let Some((controller, controller_size)) = port.reg().and_then(|mut regs| regs.next()) else {
        return;
    };
    let (clock_ids, reset_ids, perst_pin) = match (controller, port_base) {
        (0x2b000000, 0x48) => ([200, 198, 199], [139, 140, 141, 142, 143, 144], 26),
        (0x2c000000, 0x1f8) => ([203, 201, 202], [145, 146, 147, 148, 149, 150], 28),
        _ => return,
    };
    if controller_size < 0x1000 || stg_base != 0x10240000 || stg_size < port_base + 0xe8 + 4 {
        return;
    }
    let Some(soc) = tree.root().find_first_child(fdt::NodeName("soc")) else {
        return;
    };
    let Some(crg) = soc.find_first_child(fdt::NodeName("clock-controller")) else {
        return;
    };
    let Some(mut crg_regs) = crg.reg() else {
        return;
    };
    let Some((sys_crg, sys_crg_size)) = crg_regs.next() else {
        return;
    };
    let Some((stg_crg, stg_crg_size)) = crg_regs.next() else {
        return;
    };
    if !crg.is_compatible_with("starfive,jh7110-clkgen")
        || stg_crg != 0x10230000
        || stg_crg_size < 0x7c
        || sys_crg != 0x13020000
        || sys_crg_size < 0x318
    {
        return;
    }
    let Some(clocks) = port.get_prop(PropName("clocks")) else {
        return;
    };
    let Some(resets) = port.get_prop(PropName("resets")) else {
        return;
    };
    let Some(reset_node) = soc.find_first_child(fdt::NodeName("reset-controller")) else {
        return;
    };
    if !reset_node.is_compatible_with("starfive,jh7110-reset") {
        return;
    }
    let clock_words = clocks.words();
    let reset_words = resets.words();
    if clock_words.len() != 8 || reset_words.len() != 12 {
        return;
    }
    if clock_words[1].as_u32() != 96
        || tree
            .find_by_phandle(PHandle(clock_words[0].as_u32()))
            .is_none_or(|n| n.name() != crg.name())
    {
        return;
    }
    for (index, id) in clock_ids.iter().enumerate() {
        let word = 2 * (index + 1);
        if clock_words[word + 1].as_u32() != *id
            || tree
                .find_by_phandle(PHandle(clock_words[word].as_u32()))
                .is_none_or(|n| n.name() != crg.name())
        {
            return;
        }
    }
    for (index, id) in reset_ids.iter().enumerate() {
        let word = 2 * index;
        if reset_words[word + 1].as_u32() != *id
            || tree
                .find_by_phandle(PHandle(reset_words[word].as_u32()))
                .is_none_or(|n| n.name() != reset_node.name())
        {
            return;
        }
    }
    let Some(perst) = PerstPin::from_dt(tree, &soc, port, &crg, &reset_node, perst_pin) else {
        println!("JH7110 PCIe {}: PERST pin descriptor mismatch", port.name());
        return;
    };

    // SYS IOMUX clock 112 and reset 2 are described on gpio@13040000.
    let iomux_clock = (sys_crg + 112 * 4) as usize;
    update_bits(iomux_clock, 1 << 31, 1 << 31);
    if unsafe { (iomux_clock as *const u32).read_volatile() } & (1 << 31) == 0 {
        println!("JH7110 PCIe {}: SYS IOMUX clock gate failed", port.name());
        return;
    }
    update_bits((sys_crg + 0x2f8) as usize, 1 << 2, 0);
    let deadline = PlatformTimer::microseconds().saturating_add(1_000);
    let iomux_released = loop {
        let status = unsafe { ((sys_crg + 0x308) as usize as *const u32).read_volatile() };
        if status & (1 << 2) != 0 {
            break true;
        }
        if PlatformTimer::microseconds() >= deadline {
            break false;
        }
    };
    if !iomux_released {
        println!(
            "JH7110 PCIe {}: SYS IOMUX reset still asserted",
            port.name()
        );
        return;
    }
    perst.set(false);
    update_bits((stg_base + port_base + 0xe8) as usize, 1 << 8, 1 << 8);
    update_bits(
        (stg_base + port_base + 0x7c) as usize,
        (3 << 18) | (1 << 22),
        (2 << 18) | (1 << 22),
    );
    for id in clock_ids {
        let gate = (stg_crg + (id - 190) as u64 * 4) as usize;
        update_bits(gate, 1 << 31, 1 << 31);
        if unsafe { (gate as *const u32).read_volatile() } & (1 << 31) == 0 {
            return;
        }
    }
    let reset_mask = reset_ids
        .iter()
        .fold(0u32, |mask, id| mask | 1 << (id % 32));
    update_bits((stg_crg + 0x74) as usize, reset_mask, 0);
    let reset_deadline = PlatformTimer::microseconds().saturating_add(1_000);
    let status = loop {
        let status = unsafe { ((stg_crg + 0x78) as usize as *const u32).read_volatile() };
        if status & reset_mask == reset_mask
            || PlatformTimer::microseconds() >= reset_deadline
        {
            break status;
        }
    };
    if status & reset_mask != reset_mask {
        println!(
            "JH7110 PCIe {}: root-port reset still asserted status={:#010x}",
            port.name(),
            status
        );
        return;
    }

    // PLDA GEN_SETTINGS bit 0 selects Root Port mode. No configuration-space
    // access or bus mastering is attempted until the link and ATR are ready.
    update_bits((controller + 0x80) as usize, 1, 1);
    wait_us(100_000);
    perst.set(true);
    wait_us(100_000);
    let deadline = PlatformTimer::microseconds().saturating_add(200_000);
    while PlatformTimer::microseconds() < deadline {
        let state = unsafe { (link as *const u32).read_volatile() };
        if state & (1 << 5) != 0 {
            probe_plda_config(port, controller);
            return;
        }
    }
    println!("JH7110 PCIe {}: link down", port.name());
}

/// Linux's PLDA host driver programs outbound ATR slot 0 as a 256 MiB
/// configuration window. Until that slot is enabled, the second `reg`
/// resource in the DT is not an ECAM view. Probe only bus 1 device 0 after
/// link-up; leave memory windows, BARs and DMA untouched.
fn probe_plda_config(port: &Node, controller: u64) {
    let Some(mut regs) = port.reg() else { return };
    let _ = regs.next();
    let Some((config_base, config_size)) = regs.next() else {
        return;
    };
    let expected = match controller {
        0x2b000000 => 0x9_4000_0000,
        0x2c000000 => 0x9_c000_0000,
        _ => return,
    };
    if config_base != expected || config_size != 0x1000_0000 {
        return;
    }

    // The local bridge configuration header is at controller + 0x1000.
    // Assign secondary bus 1 so a config TLP can reach the linked endpoint.
    let bus_numbers = (controller + 0x1018) as usize;
    update_bits(bus_numbers, 0x00ff_ffff, 0x00ff_0100);
    let buses = unsafe { (bus_numbers as *const u32).read_volatile() };
    if buses & 0x00ff_ffff != 0x00ff_0100 {
        println!(
            "JH7110 PCIe {}: root bus numbers did not stick {:#010x}",
            port.name(),
            buses
        );
        return;
    }

    // ATR0_AXI4_SLV0: source = DT config aperture, target = PCI config
    // address zero, interface 1. The encoded size is log2(256 MiB)-1.
    let atr = controller + 0x800;
    unsafe {
        ((atr + 0x10) as usize as *mut u32).write_volatile(1);
        ((atr + 0x00) as usize as *mut u32)
            .write_volatile((config_base as u32 & !0xfff) | (27 << 1) | 1);
        ((atr + 0x04) as usize as *mut u32).write_volatile((config_base >> 32) as u32);
        ((atr + 0x08) as usize as *mut u32).write_volatile(0);
        ((atr + 0x0c) as usize as *mut u32).write_volatile(0);
    }
    let atr_source = unsafe { (atr as usize as *const u32).read_volatile() };
    if atr_source & 1 == 0 {
        println!("JH7110 PCIe {}: config ATR did not stick", port.name());
        return;
    }
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) };

    // Standard ECAM: bus 1, device 0, function 0 => +1 MiB.
    let endpoint = (config_base + 0x10_0000) as usize;
    let id = unsafe { (endpoint as *const u32).read_volatile() };
    if id == 0xffff_ffff || id == 0 {
        println!(
            "JH7110 PCIe {}: no endpoint at 01:00.0, ID {:#010x}",
            port.name(),
            id
        );
        return;
    }
    let class_revision = unsafe { ((endpoint + 8) as *const u32).read_volatile() };
    println!(
        "JH7110 PCIe {}: 01:00.0 vendor={:#06x} device={:#06x} class={:#08x}",
        port.name(),
        id & 0xffff,
        id >> 16,
        class_revision >> 8,
    );
    if id == 0x3483_1106 && class_revision >> 8 == 0x0c0330 {
        probe_vl805_mmio(port, controller, endpoint);
    }
}

/// Assign only the VL805's non-prefetchable BAR and read its xHCI capability.
/// Bus Master remains clear, so the controller cannot DMA into system RAM.
fn probe_vl805_mmio(port: &Node, controller: u64, endpoint: usize) {
    let Some(ranges) = port.get_prop(PropName::RANGES) else {
        return;
    };
    let words = ranges.words();
    if words.len() < 7
        || [
            words[0].as_u32(),
            words[1].as_u32(),
            words[2].as_u32(),
            words[3].as_u32(),
            words[4].as_u32(),
            words[5].as_u32(),
            words[6].as_u32(),
        ] != [0x8200_0000, 0, 0x3000_0000, 0, 0x3000_0000, 0, 0x0800_0000]
    {
        println!("JH7110 PCIe {}: unsupported PCI memory range", port.name());
        return;
    }
    let command = endpoint + 4;
    let old_command = unsafe { (command as *const u16).read_volatile() };
    // Memory decoding and bus mastering must both be off while sizing BAR0.
    update_pci_command(command, 0x0006, 0);
    let bar0 = endpoint + 0x10;
    let bar1 = endpoint + 0x14;
    let old_bar0 = unsafe { (bar0 as *const u32).read_volatile() };
    let old_bar1 = unsafe { (bar1 as *const u32).read_volatile() };
    if old_bar0 & 7 != 4 {
        println!(
            "JH7110 PCIe {}: VL805 BAR0 type {:#010x} unsupported",
            port.name(),
            old_bar0
        );
        update_pci_command(command, 0xffff, old_command & !4);
        return;
    }
    unsafe {
        (bar0 as *mut u32).write_volatile(u32::MAX);
        (bar1 as *mut u32).write_volatile(u32::MAX);
        core::arch::asm!("fence iorw, iorw", options(nostack));
    }
    let mask0 = unsafe { (bar0 as *const u32).read_volatile() };
    let mask1 = unsafe { (bar1 as *const u32).read_volatile() };
    unsafe {
        (bar0 as *mut u32).write_volatile(old_bar0);
        (bar1 as *mut u32).write_volatile(old_bar1);
        core::arch::asm!("fence iorw, iorw", options(nostack));
    }
    let mask = ((mask1 as u64) << 32) | (mask0 as u64 & !0xf);
    let size = (!mask).wrapping_add(1);
    if size < 0x1000 || size > 0x0800_0000 || !size.is_power_of_two() {
        println!(
            "JH7110 PCIe {}: VL805 BAR0 size {:#x} unsupported",
            port.name(),
            size
        );
        update_pci_command(command, 0xffff, old_command & !4);
        return;
    }

    // PLDA outbound ATR slot 1 maps the DT's 128 MiB CPU aperture to the
    // same PCI bus address. No inbound DMA window is programmed here.
    let atr = controller + 0x820;
    unsafe {
        ((atr + 0x10) as usize as *mut u32).write_volatile(0);
        ((atr + 0x00) as usize as *mut u32).write_volatile(0x3000_0000 | (26 << 1) | 1);
        ((atr + 0x04) as usize as *mut u32).write_volatile(0);
        ((atr + 0x08) as usize as *mut u32).write_volatile(0x3000_0000);
        ((atr + 0x0c) as usize as *mut u32).write_volatile(0);
    }
    // Root-port Type 1 memory base/limit: 0x30000000..0x37ffffff.
    update_bits((controller + 0x1020) as usize, u32::MAX, 0x37f0_3000);
    unsafe {
        (bar0 as *mut u32).write_volatile(0x3000_0000 | (old_bar0 & 0xf));
        (bar1 as *mut u32).write_volatile(0);
    }
    update_pci_command((controller + 0x1004) as usize, 0x0006, 0x0002);
    update_pci_command(command, 0x0006, 0x0002);
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) };
    let configured_bar = unsafe { (bar0 as *const u32).read_volatile() };
    let configured_command = unsafe { (command as *const u16).read_volatile() };
    if configured_bar & !0xf != 0x3000_0000 || configured_command & 0x0006 != 0x0002 {
        println!(
            "JH7110 PCIe {}: VL805 BAR0 setup failed BAR0={:#010x} command={:#06x}",
            port.name(),
            configured_bar,
            configured_command
        );
        return;
    }

    let cap = unsafe { (0x3000_0000usize as *const u32).read_volatile() };
    let hcs1 = unsafe { (0x3000_0004usize as *const u32).read_volatile() };
    if cap == 0x0100_0020 && hcs1 & 0xff != 0 {
        VL805_ENDPOINT.store(endpoint, Ordering::Release);
    } else {
        println!(
            "JH7110 PCIe {}: unexpected VL805 xHCI cap={:#010x} hcsparams1={:#010x}",
            port.name(),
            cap,
            hcs1
        );
    }
}

/// Set up the same inbound identity window as the Linux PLDA host driver,
/// then enable bus mastering for the verified VL805 only. All DMA allocations
/// are subsequently restricted to the early allocator's low RAM bank.
#[cfg(feature = "usb")]
pub(super) fn prepare_vl805_dma() -> Result<usize, &'static str> {
    let endpoint = VL805_ENDPOINT.load(Ordering::Acquire);
    if endpoint != 0x9_4010_0000 {
        return Err("verified PCIe0 VL805 not found");
    }
    let info = System::boot_info();
    let start = info.start_conventional_memory as u64;
    let end = start + info.conventional_memory_size as u64;
    if start < 0x4000_0000 || end > 0x1_0000_0000 || end <= start {
        return Err("DMA allocator outside low JH7110 RAM");
    }
    let controller = 0x2b00_0000usize;
    let inbound = controller + 0x600;
    update_bits(inbound, 0x7e, 0x25 << 1);
    unsafe { ((inbound + 4) as *mut u32).write_volatile(0) };
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) };
    let source = unsafe { (inbound as *const u32).read_volatile() };
    let source_hi = unsafe { ((inbound + 4) as *const u32).read_volatile() };
    if source & 0x7e != 0x4a || source_hi != 0 {
        return Err("PLDA inbound ATR did not stick");
    }
    update_pci_command(controller + 0x1004, 0x0004, 0x0004);
    let command = update_pci_command(endpoint + 4, 0x0004, 0x0004);
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) };
    if command & 0x0006 != 0x0006 {
        return Err("VL805 bus master could not be enabled");
    }
    Ok(0x3000_0000)
}

#[cfg(feature = "usb")]
pub(super) fn disable_vl805_dma() {
    let endpoint = VL805_ENDPOINT.load(Ordering::Acquire);
    if endpoint != 0 {
        update_pci_command(endpoint + 4, 0x0004, 0);
        update_pci_command(0x2b00_1004, 0x0004, 0);
    }
}

fn wait_us(duration: u64) {
    let deadline = PlatformTimer::microseconds().saturating_add(duration);
    while PlatformTimer::microseconds() < deadline {
        core::hint::spin_loop();
    }
}

struct PerstPin {
    gpio_base: u64,
    pin: u32,
    function_mask: u32,
}

impl PerstPin {
    fn from_dt(
        tree: &DeviceTree,
        soc: &Node,
        port: &Node,
        crg: &Node,
        reset: &Node,
        pin: u32,
    ) -> Option<Self> {
        // find_first_child matches the node name without its @unit-address.
        let gpio = soc.find_first_child(fdt::NodeName("gpio"))?;
        if !gpio.is_compatible_with("starfive,jh7110-sys-pinctrl") {
            return None;
        }
        let (gpio_base, gpio_size) = gpio.reg()?.next()?;
        if gpio_base != 0x13040000 || gpio_size < 0x2a4 {
            return None;
        }
        let gpio_clocks = gpio.get_prop(PropName("clocks"))?.words();
        let gpio_resets = gpio.get_prop(PropName("resets"))?.words();
        if gpio_clocks.len() != 2
            || gpio_resets.len() != 2
            || gpio_clocks[1].as_u32() != 112
            || gpio_resets[1].as_u32() != 2
            || tree
                .find_by_phandle(PHandle(gpio_clocks[0].as_u32()))
                .is_none_or(|n| n.name() != crg.name())
            || tree
                .find_by_phandle(PHandle(gpio_resets[0].as_u32()))
                .is_none_or(|n| n.name() != reset.name())
        {
            return None;
        }
        for (state, expected_dout) in [(1, 1), (2, 0)] {
            let prop = if state == 1 { "pinctrl-1" } else { "pinctrl-2" };
            let handles = port.get_prop(PropName(prop))?.words();
            if handles.len() != 1 {
                return None;
            }
            let group = tree.find_by_phandle(PHandle(handles[0].as_u32()))?;
            let pins = group.find_first_child(fdt::NodeName("perst-pins"))?;
            if pins.get_prop_u32(PropName("starfive,pins"))? != pin
                || pins.get_prop_u32(PropName("starfive,pin-gpio-dout"))? != expected_dout
                || pins.get_prop_u32(PropName("starfive,pin-gpio-doen"))? != 0
            {
                return None;
            }
            let mux = pins.get_prop(PropName("starfive,pinmux"))?.words();
            let expected_mask = if pin == 26 { 0x1c0000 } else { 0x7000000 };
            if mux.len() != 4
                || mux[0].as_u32() != 0x2a0
                || mux[2].as_u32() != expected_mask
                || mux[3].as_u32() != 0
            {
                return None;
            }
        }
        Some(Self {
            gpio_base,
            pin,
            function_mask: if pin == 26 { 0x1c0000 } else { 0x7000000 },
        })
    }

    fn set(&self, high: bool) {
        let shift = (self.pin % 4) * 8;
        let offset = (self.pin / 4) as u64 * 4;
        update_bits((self.gpio_base + 0x2a0) as usize, self.function_mask, 0);
        update_bits(
            (self.gpio_base + 0x40 + offset) as usize,
            0x7f << shift,
            (high as u32) << shift,
        );
        update_bits((self.gpio_base + offset) as usize, 0x3f << shift, 0);
    }
}

#[derive(Default)]
struct Inventory {
    pcie_total: usize,
    pcie_enabled: usize,
    hdmi_total: usize,
    hdmi_enabled: usize,
}

fn walk(node: &Node, depth: u8, found: &mut Inventory) {
    if depth > 5 {
        return;
    }
    let name = node.name();
    let name = name.as_str();
    let pcie = name.starts_with("pcie@")
        || name.starts_with("pci@")
        || node.is_compatible_with("starfive,jh7110-pcie")
        || node.is_compatible_with("plda,pci-xpressrich3-axi");
    let hdmi = name.starts_with("hdmi@")
        || node.is_compatible_with("starfive,jh7110-hdmi")
        || node.is_compatible_with("rockchip,rk3036-inno-hdmi");
    if depth > 0 && (pcie || hdmi) {
        let enabled = node.status_is_ok();
        if pcie {
            found.pcie_total += 1;
            found.pcie_enabled += enabled as usize;
        } else {
            found.hdmi_total += 1;
            found.hdmi_enabled += enabled as usize;
        }
    }
    for child in node.children() {
        walk(&child, depth + 1, found);
    }
}

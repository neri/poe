//! CVITEK CV181x (Sophgo SG2002, Milk-V Duo 256M) and CV180x (CV1800B,
//! Milk-V Duo) board support.
//!
//! The USB 2.0 port is a DWC2 core behind an OTG PHY whose role comes from an
//! ID override in the TOP misc registers. It is put into host mode here and
//! driven by polling. The register values follow the vendor Linux 5.10
//! (`drivers/usb/dwc2/platform.c`, `drivers/clk/cvitek/clk-cv181x.c` and
//! `clk-cv180x.c` in milkv-duo/duo-buildroot-sdk) and mainline
//! `phy-cv1800-usb2.c`. Both SoCs have the same USB node, clock gates and ID
//! override.

use alloc::boxed::Box;

use fdt::{DeviceTree, PropName};

use super::timer::PlatformTimer;
use crate::env::SystemService;
use crate::io::usb::UsbManager;
use crate::io::usb::dwc2::{DmaMap, Dwc2, Dwc2Platform};
use crate::io::usb::input::UsbTextInputMux;
use crate::{System, println, usb_println};

mod cache;

const USB_COMPATIBLE: &str = "cvitek,cv182x-usb";

/// TOP misc `REG_USB_PHY_CTRL`: bit 1 powers VBUS, bit 6 enables the ID
/// override and bit 7 is the overriding ID (0 host, 1 device).
const TOP_USB_PHY_CTRL: usize = 0x0300_0048;
const PHY_VBUS_POWER: u32 = 1 << 1;
const PHY_ID_OVERWRITE_EN: u32 = 1 << 6;
const PHY_ID_OVERWRITE_DEVICE: u32 = 1 << 7;

/// Clock generator. `CLK_EN_1` bits 28..=31 gate the AXI, APB, 125 MHz and
/// 33 kHz USB clocks and `CLK_EN_2` bit 0 the 12 MHz one. Bits 17 and 18 of
/// `CLK_BYP_0` run the 125 MHz and 12 MHz clocks from the 25 MHz oscillator
/// instead of their PLL.
const CLKGEN_CLK_EN_1: usize = 0x0300_2004;
const CLKGEN_CLK_EN_2: usize = 0x0300_2008;
const CLKGEN_CLK_BYP_0: usize = 0x0300_2030;
const CLK_EN_1_USB: u32 = 0xf << 28;
const CLK_EN_2_USB: u32 = 1;
const CLK_BYP_0_USB: u32 = (1 << 17) | (1 << 18);

/// Returns whether the device tree describes a CV181x or CV180x SoC.
pub(super) fn is_cv18xx(tree: &DeviceTree) -> bool {
    tree.root()
        .is_compatible_with_any(&["cvitek,cv181x", "cvitek,cv180x"])
}

/// Brings up the DWC2 in host mode and registers the USB manager.
pub(super) fn init_usb(tree: &DeviceTree) -> Result<(), &'static str> {
    let root = tree.root();
    let node = root
        .children()
        .find(|node| node.is_compatible_with(USB_COMPATIBLE))
        .ok_or("no DWC2 node")?;
    if !node.status_is_ok() {
        return Err("DWC2 node is disabled");
    }
    let (base, size) = node
        .reg()
        .and_then(|mut regs| regs.next())
        .ok_or("DWC2 node has no reg")?;
    if size < 0x1000 {
        return Err("DWC2 MMIO region is too small");
    }
    let base = base as usize;

    unsafe {
        let enabled_1 = update(CLKGEN_CLK_EN_1, 0, CLK_EN_1_USB);
        let enabled_2 = update(CLKGEN_CLK_EN_2, 0, CLK_EN_2_USB);
        let bypass = update(CLKGEN_CLK_BYP_0, CLK_BYP_0_USB, 0);
        let phy = update(
            TOP_USB_PHY_CTRL,
            PHY_ID_OVERWRITE_DEVICE,
            PHY_ID_OVERWRITE_EN | PHY_VBUS_POWER,
        );
        usb_println!(
            "CV181x USB: CLK_EN {:08x}->{:08x} {:08x}->{:08x} BYP {:08x}->{:08x} PHY_CTRL {:08x}->{:08x}",
            enabled_1.0,
            enabled_1.1,
            enabled_2.0,
            enabled_2.1,
            bypass.0,
            bypass.1,
            phy.0,
            phy.1
        );
    }
    // `maximum-speed = "full-speed"` in the DWC2 node keeps the port out of
    // high speed, as the generic USB binding defines it.
    let full_speed_only = node.get_prop_str(PropName::new("maximum-speed")) == Some("full-speed");
    if full_speed_only {
        usb_println!("CV181x USB: full speed only (maximum-speed)");
    }

    // Let the PHY settle on the new ID before the core is reset.
    let settle = PlatformTimer::microseconds().saturating_add(10_000);
    while PlatformTimer::microseconds() < settle {
        core::hint::spin_loop();
    }

    let platform = Dwc2Platform {
        now_us: PlatformTimer::microseconds,
        dcache_clean: cache::dcache_clean,
        dcache_invalidate: cache::dcache_invalidate,
        dcache_clean_invalidate: cache::dcache_clean_invalidate,
        // HBstLen INCR16, as the vendor kernel programs it.
        ahb_config: 7 << 1,
        utmi_width: Some(16),
        full_speed_only,
    };
    // RAM is below 4 GiB and the core sees CPU physical addresses.
    let dma = DmaMap {
        cpu_start: 0,
        bus_start: 0,
        length: 1 << 32,
    };
    let controller = unsafe { Dwc2::new(base, dma, platform) }.map_err(|error| {
        let read = |offset: usize| unsafe { ((base + offset) as *const u32).read_volatile() };
        usb_println!(
            "CV181x USB: {:?}: GOTGCTL={:08x} GUSBCFG={:08x} GRSTCTL={:08x} GINTSTS={:08x} HPRT={:08x}",
            error,
            read(0x000),
            read(0x00c),
            read(0x010),
            read(0x014),
            read(0x440)
        );
        "DWC2 initialization failed"
    })?;
    controller.log_state();

    crate::io::usb::class::msc::registry::with_global(|r| r.set_clock(PlatformTimer::microseconds));
    let mut manager = UsbManager::new(Box::new(controller), PlatformTimer::microseconds);
    let deadline = PlatformTimer::microseconds().saturating_add(3_000_000);
    while !manager.keyboard_ready() && PlatformTimer::microseconds() < deadline {
        manager.poll().map_err(|_| "USB bootstrap polling failed")?;
        core::hint::spin_loop();
    }
    if manager.keyboard_ready() {
        println!("USB keyboard ready");
    }
    System::register_service(Box::new(manager)).map_err(|_| "USB service registration failed")?;
    let fallback = System::stdin();
    let mux = Box::leak(Box::new(UsbTextInputMux::new(fallback)));
    unsafe { System::set_stdin(mux) };
    Ok(())
}

/// Clears `clear`, sets `set` and returns the register before and after.
unsafe fn update(address: usize, clear: u32, set: u32) -> (u32, u32) {
    let register = address as *mut u32;
    unsafe {
        let before = register.read_volatile();
        register.write_volatile((before & !clear) | set);
        (before, register.read_volatile())
    }
}

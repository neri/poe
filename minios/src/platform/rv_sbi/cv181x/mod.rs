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

#[cfg(feature = "usb")]
use alloc::boxed::Box;

use fdt::{DeviceTree, NodeName, PropName};

use super::timer::PlatformTimer;
#[cfg(any(feature = "usb", feature = "sd"))]
use crate::System;
#[cfg(feature = "usb")]
use crate::env::SystemService;
#[cfg(feature = "usb")]
use crate::io::usb::UsbManager;
#[cfg(feature = "usb")]
use crate::io::usb::dwc2::{DmaMap, Dwc2, Dwc2Platform};
#[cfg(feature = "usb")]
use crate::io::usb::input::UsbTextInputMux;
#[cfg(any(feature = "usb", feature = "sd"))]
use crate::println;
#[cfg(feature = "usb")]
use crate::usb_println;

#[cfg(feature = "usb")]
mod cache;

#[cfg(feature = "usb")]
const USB_COMPATIBLE: &str = "cvitek,cv182x-usb";

/// TOP misc `REG_USB_PHY_CTRL`: bit 1 powers VBUS, bit 6 enables the ID
/// override and bit 7 is the overriding ID (0 host, 1 device).
#[cfg(feature = "usb")]
const TOP_USB_PHY_CTRL: usize = 0x0300_0048;
#[cfg(feature = "usb")]
const PHY_VBUS_POWER: u32 = 1 << 1;
#[cfg(feature = "usb")]
const PHY_ID_OVERWRITE_EN: u32 = 1 << 6;
#[cfg(feature = "usb")]
const PHY_ID_OVERWRITE_DEVICE: u32 = 1 << 7;

/// Clock generator. `CLK_EN_1` bits 28..=31 gate the AXI, APB, 125 MHz and
/// 33 kHz USB clocks and `CLK_EN_2` bit 0 the 12 MHz one. Bits 17 and 18 of
/// `CLK_BYP_0` run the 125 MHz and 12 MHz clocks from the 25 MHz oscillator
/// instead of their PLL.
#[cfg(feature = "usb")]
const CLKGEN_CLK_EN_1: usize = 0x0300_2004;
#[cfg(feature = "usb")]
const CLKGEN_CLK_EN_2: usize = 0x0300_2008;
#[cfg(feature = "usb")]
const CLKGEN_CLK_BYP_0: usize = 0x0300_2030;
#[cfg(feature = "usb")]
const CLK_EN_1_USB: u32 = 0xf << 28;
#[cfg(feature = "usb")]
const CLK_EN_2_USB: u32 = 1;
#[cfg(feature = "usb")]
const CLK_BYP_0_USB: u32 = (1 << 17) | (1 << 18);

/// Returns whether the device tree describes a CV181x or CV180x SoC.
pub(super) fn is_cv18xx(tree: &DeviceTree) -> bool {
    tree.root().is_compatible_with_any(&[
        "cvitek,cv181x",
        "cvitek,cv180x",
        "sophgo,cv1800b",
        "sophgo,cv1812h",
        "milkv,duo",
        "milkv,duo256m",
    ])
}

/// Brings up the DWC2 in host mode and registers the USB manager.
#[cfg(feature = "usb")]
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

/// Initializes the native SD slot on Milk-V Duo 64M (CV1800B) or Duo 256M
/// (CV1812H/SG2002). The pinmux register offsets differ between the SoCs.
#[cfg(feature = "sd")]
pub(super) fn init_sd(tree: &DeviceTree) -> Result<(), &'static str> {
    use crate::io::sd::sdhci::Sdhci;
    use crate::io::sd::{self};

    let root = tree.root();
    let model = root.model();
    let cv1800b = root.is_compatible_with("sophgo,cv1800b")
        || root.is_compatible_with("milkv,duo")
        || model == "Milk-V Duo";
    let cv1812h = root.is_compatible_with("sophgo,cv1812h")
        || root.is_compatible_with("milkv,duo256m")
        || model == "Milk-V Duo256M";
    if !cv1800b && !cv1812h {
        return Err("SD setup supports Milk-V Duo 64M and Duo 256M only");
    }
    let oscillator = root
        .children()
        .find(|node| node.name().0.starts_with("oscillator"))
        .ok_or("CV1800B DT has no crystal oscillator node")?;
    if oscillator.get_prop_u32(PropName("clock-frequency")) != Some(25_000_000) {
        return Err("CV1800B DT does not describe the expected 25 MHz crystal");
    }
    // Mainline DTs put devices below /soc. The vendor FIT DTs used to boot
    // these boards place the same devices directly below the root node.
    let host_node = root
        .children()
        .find(|node| {
            node.is_compatible_with("sophgo,cv1800b-dwcmshc")
                || node.is_compatible_with("sophgo,sg2002-dwcmshc")
                || node.is_compatible_with("cvitek,cv180x-sd")
                || node.is_compatible_with("cvitek,cv181x-sd")
        })
        .or_else(|| {
            root.find_first_child(NodeName("soc"))?
                .children()
                .find(|node| {
                    node.is_compatible_with("sophgo,cv1800b-dwcmshc")
                        || node.is_compatible_with("sophgo,sg2002-dwcmshc")
                        || node.is_compatible_with("cvitek,cv180x-sd")
                        || node.is_compatible_with("cvitek,cv181x-sd")
                })
        })
        .ok_or("CV18xx SDHCI node is absent from the DT")?;
    let (host_base, host_size) = host_node
        .reg()
        .and_then(|mut regs| regs.next())
        .ok_or("CV18xx SDHCI has no reg property")?;
    if host_base != 0x0431_0000 || host_size < 0x1000 {
        return Err("unsupported CV18xx SDHCI register range");
    }

    let pinctrl = root
        .children()
        .find(|node| {
            node.is_compatible_with("sophgo,cv1800b-pinctrl")
                || (cv1812h && node.is_compatible_with("sophgo,cv1812h-pinctrl"))
        })
        .or_else(|| {
            root.find_first_child(NodeName("soc"))?
                .children()
                .find(|node| {
                    node.is_compatible_with("sophgo,cv1800b-pinctrl")
                        || (cv1812h && node.is_compatible_with("sophgo,cv1812h-pinctrl"))
                })
        });
    let (pin_base, pin_size) =
        match pinctrl.and_then(|node| node.reg().and_then(|mut regs| regs.next())) {
            Some(range) => range,
            // The vendor board DTBs omit pinctrl nodes; these addresses are fixed
            // by the CV1800B/CV1812H SoC memory map.
            None => (0x0300_1000, 0x1000),
        };
    if pin_base != 0x0300_1000 || pin_size < 0xa18 {
        return Err("unsupported CV18xx pinctrl register range");
    }

    let clocks = root
        .children()
        .find(|node| {
            node.is_compatible_with("sophgo,cv1800-clk")
                || node.is_compatible_with("cvitek,cv181x-clk")
                || node.is_compatible_with("cvitek,cv180x-clk")
        })
        .or_else(|| {
            root.find_first_child(NodeName("soc"))?
                .children()
                .find(|node| node.is_compatible_with("sophgo,cv1810-clk"))
        })
        .or_else(|| {
            root.find_first_child(NodeName("soc"))?
                .children()
                .find(|node| node.name().0.starts_with("clock-controller@3002000"))
        });
    let (clock_base, clock_size) =
        match clocks.and_then(|node| node.reg().and_then(|mut regs| regs.next())) {
            Some(range) => range,
            // As above, vendor DTBs omit the clock-controller description.
            None => (0x0300_2000, 0x1000),
        };
    if clock_base != 0x0300_2000 || clock_size < 0x74 {
        return Err("unsupported CV18xx clock-controller register range");
    }

    // SD0 pins use mux function 0. CV1812H assigns the SD0 mux controls 0x1c
    // bytes later than CV1800B; pad-configuration registers are shared.
    let mux_shift = if cv1812h { 0x01c } else { 0 };
    // Configure 3.3 V compatible pad pulls and drive strengths from the board
    // SD0 pin group.
    // The physical VDDIO rail on the Duo is fixed by the board design.
    unsafe {
        for (mux_offset, conf_offset, drive) in [
            (mux_shift, 0xa00, 4u32),      // CLK: 16.1 mA
            (mux_shift + 0x004, 0xa04, 2), // CMD: 10.8 mA
            (mux_shift + 0x008, 0xa08, 2), // D0
            (mux_shift + 0x00c, 0xa0c, 2), // D1
            (mux_shift + 0x010, 0xa10, 2), // D2
            (mux_shift + 0x014, 0xa14, 2), // D3
            (mux_shift + 0x018, 0x900, 2), // CD
        ] {
            write_mmio32((pin_base + mux_offset) as usize, 0);
            update_mmio32(
                (pin_base + conf_offset) as usize,
                (0x7 << 5) | (1 << 2),
                (drive << 5) | (1 << 2),
            );
        }
        // Route SD0 from the 25 MHz crystal through a divide-by-one path,
        // keeping the card bus at or below the SD default-speed limit.
        update_mmio32((clock_base + 0x30) as usize, 1 << 6, 1 << 6);
        update_mmio32((clock_base + 0x70) as usize, (0x1f << 16) | (0x3 << 8), 0);
        update_mmio32((clock_base + 0x00) as usize, 0, (1 << 18) | (1 << 19));
    }

    let host = unsafe {
        Sdhci::new_cv18xx(
            host_base as usize,
            host_size as usize,
            25_000_000,
            PlatformTimer::microseconds,
        )
    }
    .map_err(|_| "unsupported CV18xx SDHCI vendor register range")?;
    let index = match sd::register(host) {
        Ok(index) => index,
        Err(error) => {
            println!("CV18xx SD: card initialization error: {:?}", error);
            return Err("CV1800B SD card initialization failed");
        }
    };
    let info = sd::devices()
        .into_iter()
        .find(|device| device.index == index)
        .and_then(|device| device.info)
        .ok_or("CV1800B SD card was not published")?;
    println!(
        "SD: sd{} {:?} {} blocks, {}-bit at {} Hz",
        index,
        info.kind,
        info.capacity_blocks,
        if info.bus_width == sd::BusWidth::Four {
            4
        } else {
            1
        },
        info.clock_hz
    );
    Ok(())
}

/// Clears `clear`, sets `set` and returns the register before and after.
#[cfg(feature = "usb")]
unsafe fn update(address: usize, clear: u32, set: u32) -> (u32, u32) {
    let register = address as *mut u32;
    unsafe {
        let before = register.read_volatile();
        register.write_volatile((before & !clear) | set);
        (before, register.read_volatile())
    }
}

#[cfg(feature = "sd")]
unsafe fn write_mmio32(address: usize, value: u32) {
    unsafe { (address as *mut u32).write_volatile(value) }
}

#[cfg(feature = "sd")]
unsafe fn update_mmio32(address: usize, clear: u32, set: u32) {
    let register = address as *mut u32;
    unsafe {
        let value = register.read_volatile();
        register.write_volatile((value & !clear) | set);
    }
}

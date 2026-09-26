//! JH7110 DC8200 + Innosilicon HDMI bring-up with DT validation.
//! The PMU sequence completes before any VOUT or HDMI register access.
//!
//! [`init`] powers the display domain, validates the DT and allocates the
//! framebuffer once. The HDMI output itself is started by
//! [`GraphicsOutputDevice::set_mode`] and stopped again by
//! [`GraphicsOutputDevice::detach`], so it can be restarted after a switch to
//! text mode or a late monitor connection.

use alloc::boxed::Box;
use core::alloc::Layout;

use fdt::{DeviceTree, Node, NodeName, PHandle, PropName};

use super::super::timer::PlatformTimer;
use crate::io::graphics::{
    CurrentMode, GraphicsOutputDevice, ModeIndex, ModeInfo, PixelFormat, PreferredGraphicsMode,
};
use crate::mem::{MemoryManager, MemoryType};
use crate::{PhysicalAddress, System, println};

const PMU_BASE: usize = 0x1703_0000;
const SYS_CRG: usize = 0x1302_0000;
const SYS_SYSCON: usize = 0x1303_0000;
const VOUT_CRG: usize = 0x295c_0000;
const VOUT_SYSCON: usize = 0x295b_0000;
const HDMI_BASE: usize = 0x2959_0000;
const SYS_GPIO: usize = 0x1304_0000;
const DC_BASE: usize = 0x2940_0000;
const WIDTH: usize = 1920;
const HEIGHT: usize = 1080;
const VOUT: u32 = 1 << 4;

fn read(offset: usize) -> u32 {
    unsafe { ((PMU_BASE + offset) as *const u32).read_volatile() }
}

fn write(offset: usize, value: u32) {
    unsafe { ((PMU_BASE + offset) as *mut u32).write_volatile(value) }
}

/// Validate the live DT's display power-domain link before modifying PMU,
/// then register the display as a graphics output. Scanout stays off.
pub(super) fn init(tree: &DeviceTree) -> Result<(), &'static str> {
    let soc = tree
        .root()
        .find_first_child(NodeName("soc"))
        .ok_or("no SoC node")?;
    let pmu = soc
        .find_first_child(NodeName("power-controller"))
        .ok_or("no PMU node")?;
    if !pmu.status_is_ok()
        || !pmu.is_compatible_with("starfive,jh7110-pmu")
        || pmu.reg().and_then(|mut regs| regs.next()) != Some((PMU_BASE as u64, 0x10000))
    {
        return Err("unexpected PMU node");
    }
    let vout = soc
        .children()
        .find(|n| n.is_compatible_with("starfive,jh7110-clk-vout"))
        .ok_or("no VOUT clock node")?;
    if !vout.status_is_ok()
        || vout.reg().and_then(|mut regs| regs.next()) != Some((0x295c_0000, 0x10000))
    {
        return Err("unexpected VOUT clock node");
    }
    let domains = vout
        .get_prop(PropName("power-domains"))
        .ok_or("no VOUT power-domain link")?;
    let [phandle, domain] = domains.words() else {
        return Err("invalid VOUT power-domain link");
    };
    if domain.as_u32() != 4
        || tree
            .find_by_phandle(PHandle(phandle.as_u32()))
            .is_none_or(|node| node.name() != pmu.name())
    {
        return Err("VOUT power domain does not match PMU");
    }
    let dc = soc
        .children()
        .find(|n| n.is_compatible_with("starfive,jh7110-dc8200"))
        .ok_or("no DC8200 node")?;
    let hdmi = soc
        .children()
        .find(|n| n.is_compatible_with("starfive,jh7110-hdmi"))
        .ok_or("no HDMI node")?;
    if !dc.status_is_ok() || !hdmi.status_is_ok() {
        return Err("DC8200 or HDMI is disabled in boot DTB");
    }

    let before = read(0x80);
    if before & VOUT == 0 {
        // SW turn-on power mode is a command mask, not a persistent power bit.
        // The TRM specifies 0xff -> 0x05 -> 0x50 for software turn-on.
        write(0x0c, VOUT);
        write(0x44, 0xff);
        write(0x44, 0x05);
        write(0x44, 0x50);
        unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) };
        let deadline = PlatformTimer::microseconds().saturating_add(100_000);
        loop {
            let current = read(0x80);
            if current & VOUT != 0 {
                break;
            }
            if PlatformTimer::microseconds() >= deadline {
                println!(
                    "JH7110 HDMI: PMU VOUT timeout current={:#010x} event={:#010x}",
                    current,
                    read(0x88)
                );
                return Err("VOUT power-on timed out");
            }
            core::hint::spin_loop();
        }
    }
    enable_dc_clocks(tree, &soc, &dc)?;
    probe_dc_identity(&dc)?;
    probe_hdmi_routing(&soc)?;
    prepare_hdmi_pixel_clock(tree, &soc, &dc, &hdmi)?;
    prepare_hdmi_interface(tree, &soc, &hdmi)?;
    check_cache_controller(&soc)?;
    let framebuffer = alloc_framebuffer(&dc)?;
    install_graphics(framebuffer);
    Ok(())
}

/// Read the DC8200 top registers only after the display domain, bus clocks,
/// and core reset have been brought up. The top aperture is the first DT reg.
fn probe_dc_identity(dc: &Node) -> Result<(), &'static str> {
    let (base, size) = dc
        .reg()
        .and_then(|mut regs| regs.next())
        .ok_or("no DC8200 top register aperture")?;
    if (base, size) != (0x2940_0000, 0x100) {
        return Err("unexpected DC8200 top register aperture");
    }
    let base = base as usize;
    // Linux's VeriSilicon DC8200 top-register definitions give these offsets.
    // Keep this stage read-only until the identity is confirmed on hardware.
    let model = unsafe { register(base, 0x20).read_volatile() };
    let revision = unsafe { register(base, 0x24).read_volatile() };
    if model != 0x8200 || revision != 0x5720 {
        println!(
            "JH7110 HDMI: DC8200 model={:#010x} revision={:#010x}",
            model, revision
        );
        return Err("unsupported DC8200 model or revision");
    }
    Ok(())
}

/// Prepare a 148.5 MHz pixel clock for DC8200 panel 0. The VOUT mux shows
/// panel 0 selected for HDMI on this board. Scanout and PHY stay disabled.
fn prepare_hdmi_pixel_clock(
    tree: &DeviceTree,
    soc: &Node,
    dc: &Node,
    hdmi: &Node,
) -> Result<(), &'static str> {
    if hdmi.reg().and_then(|mut regs| regs.next()) != Some((HDMI_BASE as u64, 0x4000)) {
        return Err("unexpected HDMI register aperture");
    }
    let dc_port = dc
        .find_first_child(NodeName("port"))
        .ok_or("no DC8200 output port")?;
    let dc_pixel1 = dc_port
        .children()
        .find(|node| cell(node, "reg") == Some(1))
        .ok_or("no DC8200 output-1 endpoint")?;
    let hdmi_port = hdmi
        .find_first_child(NodeName("port"))
        .ok_or("no HDMI input port")?;
    let hdmi_input = hdmi_port
        .children()
        .find(|node| cell(node, "reg") == Some(0))
        .ok_or("no HDMI input endpoint")?;
    let dc_remote =
        cell(&dc_pixel1, "remote-endpoint").ok_or("DC8200 output-1 endpoint has no remote link")?;
    let hdmi_remote =
        cell(&hdmi_input, "remote-endpoint").ok_or("HDMI input endpoint has no remote link")?;
    if Some(dc_remote) != cell(&hdmi_input, "phandle")
        || Some(hdmi_remote) != cell(&dc_pixel1, "phandle")
    {
        return Err("DC8200 output 1 is not connected to HDMI");
    }
    let syscon = soc
        .children()
        .find(|node| {
            node.reg().and_then(|mut regs| regs.next()) == Some((SYS_SYSCON as u64, 0x1000))
        })
        .ok_or("no SYS syscon for PLL2")?;
    if !syscon.is_compatible_with("syscon") {
        return Err("unexpected SYS syscon for PLL2");
    }
    let osc = tree
        .root()
        .find_first_child(NodeName("osc"))
        .ok_or("no oscillator clock")?;
    if !osc.is_compatible_with("fixed-clock") || cell(&osc, "clock-frequency") != Some(24_000_000) {
        return Err("unexpected oscillator frequency");
    }

    let pd_fbdiv = unsafe { register(SYS_SYSCON, 0x2c).read_volatile() };
    let frac_postdiv = unsafe { register(SYS_SYSCON, 0x30).read_volatile() };
    let prediv_reg = unsafe { register(SYS_SYSCON, 0x34).read_volatile() };
    let pd = (pd_fbdiv >> 15) & 3;
    let fbdiv = (pd_fbdiv >> 17) & 0xfff;
    let frac = frac_postdiv & 0x00ff_ffff;
    let postdiv = (frac_postdiv >> 28) & 3;
    let prediv = prediv_reg & 0x3f;
    if (pd != 0 && pd != 3) || fbdiv == 0 || prediv == 0 {
        return Err("invalid PLL2 configuration");
    }
    let numerator = 24_000_000u64 * fbdiv as u64
        + if pd == 0 {
            (24_000_000u64 * frac as u64) >> 24
        } else {
            0
        };
    let pll2_hz = numerator / ((prediv as u64) << postdiv);
    if pll2_hz != 1_188_000_000 {
        println!("JH7110 HDMI: PLL2 {} Hz", pll2_hz);
        return Err("PLL2 cannot supply 1080p60 pixel clock");
    }

    let clocks = dc.get_prop(PropName("clocks")).ok_or("no DC8200 clocks")?;
    let words = clocks.words();
    let pixel0 = 4 * 2;
    if words.len() <= pixel0 + 1 || words[pixel0 + 1].as_u32() != 7 {
        return Err("unexpected DC8200 pixel0 clock ID");
    }
    let provider = tree
        .find_by_phandle(PHandle(words[pixel0].as_u32()))
        .ok_or("DC8200 pixel0 clock provider missing")?;
    if provider.reg().and_then(|mut regs| regs.next()) != Some((VOUT_CRG as u64, 0x10000)) {
        return Err("unexpected DC8200 pixel0 clock provider");
    }

    // VOUT clock 1 is a divider; clock 7 is the gated panel-0 mux.
    // Mux value 0 selects the divider rather than the fixed 297 MHz input.
    let divider_after = set_bits(VOUT_CRG, 4, 0x00ff_ffff, 8);
    let pixel0_after = set_bits(VOUT_CRG, 7 * 4, (1 << 31) | (1 << 24), 1 << 31);
    if divider_after & 0x00ff_ffff != 8 || pixel0_after & ((1 << 31) | (1 << 24)) != 1 << 31 {
        return Err("DC8200 pixel0 clock configuration did not stick");
    }
    Ok(())
}

fn cell(node: &Node, property: &str) -> Option<u32> {
    node.get_prop(PropName(property))
        .and_then(|prop| prop.words().first().map(|word| word.as_u32()))
}

fn probe_hdmi_routing(soc: &Node) -> Result<(), &'static str> {
    let syscon = soc
        .children()
        .find(|node| {
            node.reg().and_then(|mut regs| regs.next()) == Some((VOUT_SYSCON as u64, 0x90))
        })
        .ok_or("no VOUT display syscon")?;
    if !syscon.is_compatible_with("syscon") {
        return Err("unexpected VOUT display syscon");
    }
    let panel = unsafe { register(VOUT_SYSCON, 0x08).read_volatile() };
    if panel & (1 << 4) != 0 {
        return Err("HDMI is routed from unsupported DC8200 panel 1");
    }
    Ok(())
}

/// Clock and reset the HDMI register interface and route the HPD pin.
/// The Innosilicon controller spaces each byte register four bytes apart.
fn prepare_hdmi_interface(tree: &DeviceTree, soc: &Node, hdmi: &Node) -> Result<(), &'static str> {
    let clocks = hdmi.get_prop(PropName("clocks")).ok_or("no HDMI clocks")?;
    let words = clocks.words();
    if words.len() < 2 || words[1].as_u32() != 17 {
        return Err("unexpected HDMI system clock ID");
    }
    let provider = tree
        .find_by_phandle(PHandle(words[0].as_u32()))
        .ok_or("HDMI system clock provider missing")?;
    if provider.reg().and_then(|mut regs| regs.next()) != Some((VOUT_CRG as u64, 0x10000)) {
        return Err("unexpected HDMI system clock provider");
    }
    let resets = hdmi.get_prop(PropName("resets")).ok_or("no HDMI reset")?;
    let reset_words = resets.words();
    if reset_words.len() != 2 || reset_words[1].as_u32() != 233 {
        return Err("unexpected HDMI reset ID");
    }
    let reset_provider = tree
        .find_by_phandle(PHandle(reset_words[0].as_u32()))
        .ok_or("HDMI reset provider missing")?;
    let expected_reset = soc
        .find_first_child(NodeName("reset-controller"))
        .ok_or("no reset controller")?;
    if reset_provider.name() != expected_reset.name() {
        return Err("unexpected HDMI reset provider");
    }
    let apb_reg = unsafe { register(VOUT_CRG, 0).read_volatile() };
    if apb_reg & 0x00ff_ffff == 0 {
        return Err("VOUT APB divider is zero");
    }
    gate(VOUT_CRG, 17, "HDMI sys")?;
    release_reset(VOUT_CRG, 224, 233)?;
    configure_hdmi_hpd_pin(tree, soc, hdmi)
}

fn hotplug_detected() -> bool {
    phy_read(0xc8) & (1 << 7) != 0
}

fn phy_read(index: usize) -> u32 {
    unsafe { register(HDMI_BASE, index * 4).read_volatile() }
}

fn phy_write(index: usize, value: u32) {
    unsafe { register(HDMI_BASE, index * 4).write_volatile(value) }
}

/// The controller's analog reset also resets the PHY PLLs. This must run
/// before either PLL is programmed, matching the Innosilicon probe order.
fn init_hdmi_controller() {
    set_bits(HDMI_BASE, 0, (1 << 5) | (1 << 6), 1 << 5);
    wait_us(100);
    set_bits(HDMI_BASE, 0, 1 << 6, 1 << 6);
    wait_us(100);
    set_bits(
        HDMI_BASE,
        0,
        (1 << 4) | (1 << 2) | (1 << 1) | 1,
        (1 << 4) | (1 << 2) | (1 << 1) | 1,
    );
}

/// Set the Innosilicon pre-PLL to the documented 148.5 MHz 1080p mode.
/// TMDS drivers and DC8200 scanout are still disabled here.
fn prepare_hdmi_phy_clock() -> Result<(), &'static str> {
    // The JH7110 PHY is at register index 0x100 after the HDMI controller.
    // 24 MHz * 99 / (4 * 2 * 2) = 148.5 MHz for the selected divider path.
    set_bits(HDMI_BASE, 0x1b0 * 4, 1 << 2, 1 << 2); // bias
    phy_write(0x1cc, 0x0f); // RX reference
    set_bits(HDMI_BASE, 0x1a0 * 4, 1, 1); // pre-PLL power down
    set_bits(HDMI_BASE, 0x1a0 * 4, 1 << 1, 0); // no VCO / 5
    for (index, value) in [
        (0x1a1, 0x01), // predivider
        (0x1a2, 0x70), // integer feedback, spread spectrum disabled
        (0x1a3, 0x63), // feedback divider 99
        (0x1a5, 0x41), // pixel dividers A=1, B=2
        (0x1a6, 0x42), // pixel dividers C=2, D=2
        (0x1a4, 0x15), // TMDS dividers A=B=C=1
        (0x1d3, 0x00),
        (0x1d2, 0x00),
        (0x1d1, 0x00),
    ] {
        phy_write(index, value);
    }
    set_bits(HDMI_BASE, 0x1a0 * 4, 1, 0); // pre-PLL power up
    let deadline = PlatformTimer::microseconds().saturating_add(100_000);
    let lock = loop {
        let value = phy_read(0x1a9);
        if value & 1 != 0 || PlatformTimer::microseconds() >= deadline {
            break value;
        }
        core::hint::spin_loop();
    };
    if lock & 1 == 0 {
        set_bits(HDMI_BASE, 0x1a0 * 4, 1, 1);
        return Err("HDMI PHY pre-PLL did not lock");
    }
    let after = set_bits(VOUT_CRG, 7 * 4, 1 << 24, 1 << 24);
    if after & (1 << 24) == 0 {
        return Err("DC8200 pixel0 PHY clock mux did not stick");
    }
    // The display subsystem also gates its output clock independently.
    let lcd_after = set_bits(VOUT_CRG, 9 * 4, (1 << 31) | (1 << 24), 1 << 31);
    if lcd_after & (1 << 31) == 0 {
        return Err("VOUT display output clock did not start");
    }
    prepare_hdmi_post_pll()?;
    Ok(())
}

/// Prepare the TMDS post-PLL before enabling the analog output. The Linux
/// JH7110 PHY table selects prediv=1, fbdiv=20 and postdiv=1 for 148.5 MHz.
fn prepare_hdmi_post_pll() -> Result<(), &'static str> {
    phy_write(0x1ab, 1);
    phy_write(0x1ac, 20);
    phy_write(0x1ad, 1);
    phy_write(0x1aa, 0x0e); // TMDS reference, post divider enabled, power up
    let deadline = PlatformTimer::microseconds().saturating_add(100_000);
    let lock = loop {
        let value = phy_read(0x1af);
        if value & 1 != 0 || PlatformTimer::microseconds() >= deadline {
            break value;
        }
        core::hint::spin_loop();
    };
    if lock & 1 == 0 {
        set_bits(HDMI_BASE, 0x1aa * 4, 1, 1);
        return Err("HDMI PHY post-PLL did not lock");
    }
    Ok(())
}

/// Allocate the panel 0 framebuffer. It stays allocated for as long as
/// DC8200 may read it, and starts out black in DRAM.
fn alloc_framebuffer(dc: &Node) -> Result<usize, &'static str> {
    let mut regs = dc.reg().ok_or("no DC8200 registers")?;
    let _top = regs.next().ok_or("no DC8200 top aperture")?;
    if regs.next() != Some((0x2940_0800, 0x2000)) {
        return Err("unexpected DC8200 display register aperture");
    }
    let bytes = WIDTH * HEIGHT * 4;
    let layout =
        Layout::from_size_align(bytes, 4096).map_err(|_| "invalid display buffer layout")?;
    let framebuffer = MemoryManager::zalloc(layout, None, MemoryType::Used, None)
        .map_err(|_| "cannot allocate display buffer")?;
    let address = framebuffer as usize;
    if address
        .checked_add(bytes)
        .is_none_or(|end| end > u32::MAX as usize)
    {
        unsafe { MemoryManager::zfree(framebuffer, layout) }.ok();
        return Err("display buffer is outside 32-bit DMA range");
    }
    // The zeroes may still be dirty in the cache.
    ccache_flush_range(address, bytes);
    Ok(address)
}

/// Bring up the HDMI controller, PHY PLLs, DC8200 panel 0 scanout and TMDS
/// output for 1080p60. The caller stops everything with [`stop_output`] on
/// failure.
fn start_output(address: usize) -> Result<(), &'static str> {
    init_hdmi_controller();
    prepare_hdmi_phy_clock()?;
    // Let pixel0 settle on the PHY clock before programming DC8200. Without
    // this gap the monitor sees TMDS briefly and then reports no signal.
    wait_us(20_000);

    // CEA-861 1080p60: 148.5 MHz, 2200x1125 total, positive sync.
    dc_write(0x1430, 1920 | (2200 << 16));
    dc_write(0x1438, 2008 | (2052 << 15) | (1 << 30));
    dc_write(0x1440, 1080 | (1125 << 16));
    dc_write(0x1448, 1084 | (1089 << 15) | (1 << 30));

    // The boot DT graph routes panel 0's DPI output to HDMI. Supply RGB888.
    set_bits(VOUT_SYSCON, 4, (3 << 26) | (3 << 28) | (1 << 30), 3 << 26);
    dc_write(0x1400, address as u32);
    dc_write(0x1408, (WIDTH * 4) as u32);
    dc_write(0x1518, 5 << 26); // X8R8G8B8, ARGB swizzle, linear
    dc_write(0x24d8, 0);
    dc_write(0x24e0, (WIDTH as u32) | ((HEIGHT as u32) << 15));
    dc_write(0x1810, (WIDTH as u32) | ((HEIGHT as u32) << 15));
    dc_write(0x2510, 1 << 1); // no blending
    set_bits(DC_BASE, 0x1cc0, (1 << 13) | (1 << 19), 1 << 13);
    set_bits(DC_BASE, 0x1cc0, 1 << 12, 1 << 12);
    dc_write(0x14b8, 5); // DPI RGB888
    set_bits(DC_BASE, 0x1cd0, 1 << 3, 0); // DP off
    set_bits(DC_BASE, 0x1418, (1 << 1) | (1 << 5) | (1 << 9), 0);
    set_bits(
        DC_BASE,
        0x1418,
        (1 << 0) | (1 << 4) | (1 << 8) | (1 << 12),
        (1 << 0) | (1 << 4) | (1 << 8) | (1 << 12),
    );
    set_bits(DC_BASE, 0x1ccc, (1 << 0) | (1 << 3), 1 << 0);
    set_bits(DC_BASE, 0x2518, 1, 1);

    // Innosilicon controller uses byte register numbers with a four-byte stride.
    // Its analog reset was released before programming the PHY PLLs.
    phy_write(0x01, 1); // SDR RGB444, external data enable
    phy_write(0x02, 3 << 4); // RGB, eight bits per component
    phy_write(0x03, 1); // no CSC or C0/C2 swap
    phy_write(0x04, (1 << 4) | (1 << 3)); // color depth unspecified, SOF off
    phy_write(0x05, 0x03); // audio mute and video black during setup
    phy_write(0x52, 1 << 1); // HDMI mode, no HDCP
    phy_write(0x08, (1 << 3) | (1 << 2) | 1); // external timing, +H/+V
    for (index, value) in [
        (0x09, 2200),
        (0x0b, 280),
        (0x0d, 192),
        (0x0f, 44),
        (0x11, 1125),
    ] {
        phy_write(index, value & 0xff);
        phy_write(index + 1, value >> 8);
    }
    phy_write(0x13, 45); // vertical blank
    phy_write(0x14, 41); // vertical sync delay
    phy_write(0x15, 5); // vertical sync width

    let pre_lock = phy_read(0x1a9);
    let post_lock = phy_read(0x1af);
    if pre_lock & 1 == 0 || post_lock & 1 == 0 {
        println!(
            "JH7110 HDMI: PHY lock lost before TMDS pre={:#010x} post={:#010x}",
            pre_lock, post_lock
        );
        return Err("HDMI PHY PLL lock lost before TMDS enable");
    }

    // Post-PLL is already locked. Enable analog outputs only after the DC
    // plane and both sets of timing registers have been committed.
    phy_write(0x1b4, 0x07); // LDO
    phy_write(0x1be, 0x71); // serializer
    phy_write(0x1b2, 0x8f); // TMDS clock/data drivers
    set_bits(HDMI_BASE, 0x05 * 4, 0x03, 1 << 1); // show video, mute audio
    set_bits(HDMI_BASE, 0, 1 << 1, 0); // leave standby after setup
    wait_us(30_000);
    if phy_read(0x1a9) & 1 == 0 || phy_read(0x1af) & 1 == 0 {
        phy_write(0x1b2, 0);
        set_bits(HDMI_BASE, 0, 1 << 1, 1 << 1);
        return Err("HDMI PHY PLL lock lost after output enable");
    }
    Ok(())
}

/// Reverse of [`start_output`]: blank and stop TMDS, stop panel 0, return
/// its pixel clock to the VOUT divider, then power down both PHY PLLs.
fn stop_output() {
    set_bits(HDMI_BASE, 0x05 * 4, 0x03, 0x03); // mute audio and video
    phy_write(0x1b2, 0); // TMDS clock/data drivers off
    set_bits(HDMI_BASE, 0, 1 << 1, 1 << 1); // standby
    set_bits(DC_BASE, 0x1ccc, 1 << 0, 0); // panel 0 output stop
    set_bits(VOUT_CRG, 7 * 4, 1 << 24, 0); // pixel0 <- VOUT divider
    set_bits(HDMI_BASE, 0x1aa * 4, 1, 1); // post-PLL power down
    set_bits(HDMI_BASE, 0x1a0 * 4, 1, 1); // pre-PLL power down
}

struct JH7110Graphics {
    modes: [ModeInfo; 1],
    current: CurrentMode,
    /// Whether HDMI is currently scanning out the framebuffer.
    active: bool,
}

impl GraphicsOutputDevice for JH7110Graphics {
    fn modes(&self) -> &[ModeInfo] {
        &self.modes
    }

    fn current_mode(&self) -> &CurrentMode {
        &self.current
    }

    fn preferred_graphics_mode(&self) -> Option<PreferredGraphicsMode> {
        Some(self.modes[0].into())
    }

    fn set_mode(&mut self, mode: ModeIndex) -> Result<(), ()> {
        if mode != ModeIndex(0) {
            return Err(());
        }
        if self.active {
            return Ok(());
        }
        if !hotplug_detected() {
            println!("JH7110 HDMI: no monitor detected");
            return Err(());
        }
        if let Err(reason) = start_output(self.current.fb.as_usize()) {
            stop_output();
            println!("JH7110 HDMI: output failed: {}", reason);
            return Err(());
        }
        self.active = true;
        println!("JH7110 HDMI: {}x{}@60 output enabled", WIDTH, HEIGHT);
        Ok(())
    }

    fn detach(&mut self) {
        let address = self.current.fb.as_usize();
        let size = self.current.fb_size;
        unsafe {
            (address as *mut u8).write_bytes(0, size);
        }
        // DC8200 scans out from DRAM, so the cleared lines must be written back.
        ccache_flush_range(address, size);
        if self.active {
            stop_output();
            self.active = false;
        }
    }

    fn framebuffer_sync(&self) -> Option<fn(usize, usize)> {
        Some(ccache_flush_range)
    }
}

fn install_graphics(address: usize) {
    let info = ModeInfo {
        width: WIDTH as u16,
        height: HEIGHT as u16,
        bytes_per_scanline: (WIDTH * 4) as u16,
        pixel_format: PixelFormat::BGRX8888,
    };
    let graphics = Box::new(JH7110Graphics {
        modes: [info],
        current: CurrentMode {
            current: ModeIndex(0),
            info,
            fb: PhysicalAddress::from_usize(address),
            fb_size: WIDTH * HEIGHT * 4,
        },
        active: false,
    });
    System::conctl().set_graphics(graphics);
}

/// DC8200 framebuffer DMA is noncoherent with the U74 caches, so every
/// framebuffer update is written back through SiFive CCACHE's Flush64
/// register. Check the controller once before relying on that address.
fn check_cache_controller(soc: &Node) -> Result<(), &'static str> {
    let cache = soc
        .find_first_child(NodeName("cache-controller"))
        .ok_or("no SiFive cache controller")?;
    if !cache.is_compatible_with("sifive,fu740-c000-ccache")
        || cache.reg().and_then(|mut regs| regs.next()) != Some((0x0201_0000, 0x4000))
        || cell(&cache, "cache-block-size") != Some(64)
    {
        return Err("unexpected SiFive cache controller");
    }
    Ok(())
}

fn ccache_flush_range(address: usize, bytes: usize) {
    let flush64 = 0x0201_0200 as *mut u64;
    let start = address & !63;
    let end = address.saturating_add(bytes);
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) };
    for line in (start..end).step_by(64) {
        unsafe { flush64.write_volatile(line as u64) };
    }
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) };
}

fn dc_write(offset: usize, value: u32) {
    unsafe { register(DC_BASE, offset).write_volatile(value) }
}

fn wait_us(delay: u64) {
    let deadline = PlatformTimer::microseconds().saturating_add(delay);
    while PlatformTimer::microseconds() < deadline {
        core::hint::spin_loop();
    }
}

fn configure_hdmi_hpd_pin(tree: &DeviceTree, soc: &Node, hdmi: &Node) -> Result<(), &'static str> {
    let gpio = soc
        .find_first_child(NodeName("gpio"))
        .ok_or("no SYS GPIO controller")?;
    if !gpio.is_compatible_with("starfive,jh7110-sys-pinctrl")
        || gpio.reg().and_then(|mut regs| regs.next()) != Some((SYS_GPIO as u64, 0x10000))
    {
        return Err("unexpected SYS GPIO controller");
    }
    let handles = hdmi
        .get_prop(PropName("pinctrl-0"))
        .ok_or("no HDMI pinctrl state")?
        .words();
    if handles.len() != 1 {
        return Err("unexpected HDMI pinctrl state");
    }
    let group = tree
        .find_by_phandle(PHandle(handles[0].as_u32()))
        .ok_or("HDMI pinctrl group missing")?;
    let expected_group = gpio
        .find_first_child(NodeName("inno_hdmi-pins"))
        .ok_or("no HDMI pin group in SYS GPIO")?;
    if group.name() != expected_group.name() {
        return Err("HDMI pinctrl group is not on SYS GPIO");
    }
    let hpd = group
        .find_first_child(NodeName("inno_hdmi-hpd-pins"))
        .ok_or("no HDMI HPD pin description")?;
    if cell(&hpd, "starfive,pins") != Some(15)
        || cell(&hpd, "starfive,pin-ioconfig") != Some(1)
        || cell(&hpd, "starfive,pin-gpio-doen") != Some(1)
        || cell(&hpd, "starfive,pin-gpio-din") != Some(8)
    {
        return Err("unexpected HDMI HPD pin description");
    }
    gate(SYS_CRG, 112, "SYS iomux")?;
    release_reset(SYS_CRG, 0, 2)?;

    // GPIO15 is input signal 8 (HDMI HPD). The pinctrl block stores four
    // 8-bit pin fields per word and four input selectors per GPI word.
    set_bits(SYS_GPIO, 0x0c, 0x3f << 24, 1 << 24); // DOEN15: input
    set_bits(SYS_GPIO, 0x88, 0x7f, 17); // GPI8 <- GPIO15 + 2
    set_bits(SYS_GPIO, 0x29c, 0x7 << 17, 0); // GPIO15 function 0
    set_bits(SYS_GPIO, 0x15c, 0xff, 1); // GPIO15 input enable
    Ok(())
}

fn register(base: usize, offset: usize) -> *mut u32 {
    (base + offset) as *mut u32
}

fn set_bits(base: usize, offset: usize, mask: u32, value: u32) -> u32 {
    let ptr = register(base, offset);
    let previous = unsafe { ptr.read_volatile() };
    let next = (previous & !mask) | (value & mask);
    unsafe { ptr.write_volatile(next) };
    next
}

fn gate(base: usize, id: usize, name: &str) -> Result<(), &'static str> {
    let offset = id * 4;
    set_bits(base, offset, 1 << 31, 1 << 31);
    let after = unsafe { register(base, offset).read_volatile() };
    if after & (1 << 31) == 0 {
        println!("JH7110 HDMI: {} clock {} gate did not stick", name, id);
        return Err("display clock gate did not stick");
    }
    Ok(())
}

fn release_reset(base: usize, first_id: usize, id: usize) -> Result<(), &'static str> {
    let index = id.checked_sub(first_id).ok_or("invalid display reset ID")?;
    let bank = index / 32;
    let mask = 1u32 << (index % 32);
    let assert_offset = if base == SYS_CRG { 0x2f8 } else { 0x48 };
    let status_offset = if base == SYS_CRG { 0x308 } else { 0x4c };
    set_bits(base, assert_offset + bank * 4, mask, 0);
    let deadline = PlatformTimer::microseconds().saturating_add(1_000);
    loop {
        let status = unsafe { register(base, status_offset + bank * 4).read_volatile() };
        if status & mask != 0 {
            return Ok(());
        }
        if PlatformTimer::microseconds() >= deadline {
            println!("JH7110 HDMI: reset {} release timed out", id);
            return Err("display reset release timed out");
        }
        core::hint::spin_loop();
    }
}

/// Enable only the DC8200 bus and core clocks. Pixel timing and the HDMI
/// transmitter stay untouched until a fixed mode is selected and tested.
fn enable_dc_clocks(tree: &DeviceTree, soc: &Node, dc: &Node) -> Result<(), &'static str> {
    let sys = soc
        .find_first_child(NodeName("clock-controller"))
        .ok_or("no SYS clock controller")?;
    if !sys.is_compatible_with("starfive,jh7110-clkgen")
        || sys.reg().and_then(|mut regs| regs.next()) != Some((SYS_CRG as u64, 0x10000))
    {
        return Err("unexpected SYS clock controller");
    }
    let reset = soc
        .find_first_child(NodeName("reset-controller"))
        .ok_or("no reset controller")?;
    if !reset.is_compatible_with("starfive,jh7110-reset") {
        return Err("unexpected reset controller");
    }
    let clocks = dc.get_prop(PropName("clocks")).ok_or("no DC8200 clocks")?;
    let words = clocks.words();
    // The first four are SYS clock IDs; the AXI, core and AHB clocks are
    // VOUT IDs 4..6. Validate providers as well as numeric IDs.
    for (index, id, provider_base) in [
        (0, 60, SYS_CRG as u64),
        (1, 58, SYS_CRG as u64),
        (2, 62, SYS_CRG as u64),
        (3, 61, SYS_CRG as u64),
        (6, 4, VOUT_CRG as u64),
        (7, 5, VOUT_CRG as u64),
        (8, 6, VOUT_CRG as u64),
    ] {
        let pair = index * 2;
        if words.len() <= pair + 1 || words[pair + 1].as_u32() != id {
            return Err("unexpected DC8200 clock IDs");
        }
        let Some(node) = tree.find_by_phandle(PHandle(words[pair].as_u32())) else {
            return Err("DC8200 clock provider missing");
        };
        if node
            .reg()
            .and_then(|mut regs| regs.next())
            .map(|(base, _)| base)
            != Some(provider_base)
        {
            return Err("unexpected DC8200 clock provider");
        }
    }
    let resets = dc.get_prop(PropName("resets")).ok_or("no DC8200 resets")?;
    let ids = [43, 224, 225, 226, 26];
    let words = resets.words();
    if words.len() != ids.len() * 2 {
        return Err("unexpected DC8200 reset count");
    }
    for (index, id) in ids.iter().enumerate() {
        if words[index * 2 + 1].as_u32() != *id
            || tree
                .find_by_phandle(PHandle(words[index * 2].as_u32()))
                .is_none_or(|node| node.name() != reset.name())
        {
            return Err("unexpected DC8200 reset IDs");
        }
    }

    gate(SYS_CRG, 58, "SYS vout-src")?;
    gate(SYS_CRG, 61, "SYS vout-ahb")?;
    release_reset(SYS_CRG, 0, 43)?;
    gate(SYS_CRG, 62, "SYS vout-axi")?;
    gate(SYS_CRG, 60, "SYS noc-disp")?;
    release_reset(SYS_CRG, 0, 26)?;
    for (id, label) in [(4, "DC axi"), (5, "DC core"), (6, "DC ahb")] {
        gate(VOUT_CRG, id, label)?;
    }
    for id in [224, 225, 226] {
        release_reset(VOUT_CRG, 224, id)?;
    }
    Ok(())
}

//! VisionFive 2 SDIO1 clock, pinmux, reset and DW-MMC setup.

use fdt::{DeviceTree, NodeName, PHandle, PropName};

use super::super::timer::PlatformTimer;
use crate::{System, println};

const SYS_CRG: u64 = 0x1302_0000;
const SYS_SYSCON: u64 = 0x1303_0000;
const SYS_GPIO: u64 = 0x1304_0000;
const DW_MMC: u64 = 0x1602_0000;
const INPUT_CLOCK_MAX: u64 = 50_000_000;
const PLL2_HZ_EXPECTED: u64 = 1_188_000_000;

pub(super) fn init(tree: &DeviceTree) -> Result<(), &'static str> {
    let root = tree.root();
    if !super::is_visionfive2(tree) {
        return Err("not a VisionFive 2 DT");
    }
    let soc = root
        .find_first_child(NodeName("soc"))
        .ok_or("no JH7110 SoC node")?;
    let host = soc
        .children()
        .find(|node| {
            (node.is_compatible_with("starfive,jh7110-sdio")
                || node.is_compatible_with("starfive,jh7110-mmc"))
                && node
                    .reg()
                    .and_then(|mut regs| regs.next())
                    .is_some_and(|(base, _)| base == DW_MMC)
        })
        .ok_or("SDIO1 controller is absent from the DT")?;
    let (host_base, host_size) = host
        .reg()
        .and_then(|mut regs| regs.next())
        .ok_or("SDIO1 controller has no register range")?;
    if host_base != DW_MMC || host_size < 0x1000 {
        println!(
            "JH7110 SD: unexpected SDIO1 register range: base={:#x} size={:#x}",
            host_base, host_size
        );
        return Err("unexpected SDIO1 register range");
    }
    let clocks = host
        .get_prop(PropName("clocks"))
        .ok_or("SDIO1 has no clock descriptors")?
        .words();
    if clocks.len() < 4 || clocks[1].as_u32() != 92 || clocks[3].as_u32() != 94 {
        return Err("SDIO1 clock descriptors do not match JH7110 SYSCRG");
    }
    let clock_provider = tree
        .find_by_phandle(PHandle(clocks[0].as_u32()))
        .ok_or("SDIO1 clock provider is missing")?;
    if !clock_provider.is_compatible_with("starfive,jh7110-clkgen")
        || clock_provider.reg().and_then(|mut regs| regs.next()) != Some((SYS_CRG, 0x10000))
    {
        return Err("unexpected JH7110 SYS clock controller");
    }
    let reset_words = host
        .get_prop(PropName("resets"))
        .ok_or("SDIO1 has no reset descriptor")?
        .words();
    if reset_words.len() != 2 || reset_words[1].as_u32() != 65 {
        return Err("SDIO1 reset descriptor does not match JH7110 SYSCRG");
    }
    let reset_provider = tree
        .find_by_phandle(PHandle(reset_words[0].as_u32()))
        .ok_or("SDIO1 reset provider is missing")?;
    if !reset_provider.is_compatible_with("starfive,jh7110-reset") {
        return Err("unexpected JH7110 reset controller");
    }

    let (syscon_base, syscon_size) = soc
        .children()
        .find(|node| {
            node.is_compatible_with("syscon")
                && node
                    .reg()
                    .and_then(|mut regs| regs.next())
                    .is_some_and(|(base, _)| base == SYS_SYSCON)
        })
        .and_then(|n| n.reg().and_then(|mut regs| regs.next()))
        .ok_or("JH7110 SYS syscon is missing")?;
    if syscon_base != SYS_SYSCON || syscon_size < 0x38 {
        return Err("unexpected JH7110 SYS syscon register range");
    }

    let osc = root
        .find_first_child(NodeName("osc"))
        .ok_or("JH7110 oscillator node is missing")?;
    if !osc.is_compatible_with("fixed-clock")
        || osc.get_prop_u32(PropName("clock-frequency")) != Some(24_000_000)
    {
        return Err("unexpected JH7110 oscillator frequency");
    }
    let pll2_hz = pll2_rate(24_000_000)?;
    if pll2_hz != PLL2_HZ_EXPECTED {
        return Err("JH7110 PLL2 rate is outside the supported VF2 configuration");
    }
    let bus_root = read(SYS_CRG, 5 * 4);
    let bus_parent_hz = match (bus_root >> 24) & 0xf {
        0 => 24_000_000,
        1 => pll2_hz,
        _ => return Err("unsupported JH7110 bus-root parent"),
    };
    let axi_cfg0 = read(SYS_CRG, 7 * 4);
    let axi_div = (axi_cfg0 & 0x00ff_ffff) as u64;
    if axi_div == 0 {
        return Err("JH7110 AXI clock divider is stopped");
    }
    let axi_cfg0_hz = bus_parent_hz / axi_div;
    if axi_cfg0_hz < INPUT_CLOCK_MAX || axi_cfg0_hz.div_ceil(INPUT_CLOCK_MAX) > 15 {
        return Err("AXI clock cannot provide a safe SDIO1 input clock");
    }
    let sd_div = axi_cfg0_hz.div_ceil(INPUT_CLOCK_MAX).max(1);
    let input_clock_hz = (axi_cfg0_hz / sd_div) as u32;

    let gpio = soc
        .find_first_child(NodeName("gpio"))
        .ok_or("SYS GPIO pin controller is missing")?;
    if !gpio.is_compatible_with("starfive,jh7110-sys-pinctrl")
        || gpio.reg().and_then(|mut regs| regs.next()) != Some((SYS_GPIO, 0x10000))
    {
        return Err("unexpected SYS GPIO pin controller");
    }
    let pinctrl_handles = host
        .get_prop(PropName("pinctrl-0"))
        .ok_or("SDIO1 pinctrl state is missing")?
        .words();
    if pinctrl_handles.len() != 1 {
        return Err("unexpected SDIO1 pinctrl state");
    }
    let pin_group = tree
        .find_by_phandle(PHandle(pinctrl_handles[0].as_u32()))
        .ok_or("SDIO1 pin group is missing")?;
    if gpio
        .children()
        .all(|child| child.name() != pin_group.name())
    {
        return Err("SDIO1 pin group is not owned by SYS GPIO");
    }

    gate_clock(112)?; // SYS IOMUX APB
    release_reset(2)?; // SYS IOMUX APB
    // The VisionFive 2 vendor DT uses a per-pin four-cell pinmux descriptor
    // plus separate DOUT/DOEN/DIN selectors (not the packed upstream format).
    // Validate the full group before touching pin registers.
    let mut seen_pins = 0u16;
    let mut configured = 0usize;
    for pin in pin_group.children() {
        let pad = pin
            .get_prop_u32(PropName("starfive,pins"))
            .ok_or("SDIO1 pin has no pad number")? as usize;
        let (expected_offset, expected_shift, expected_mask) = expected_pinmux(pad)
            .ok_or("SDIO1 pin is outside the supported VisionFive 2 routing")?;
        let mux = pin
            .get_prop(PropName("starfive,pinmux"))
            .ok_or("SDIO1 pin has no pinmux value")?
            .words();
        if mux.len() != 4
            || mux[0].as_u32() as usize != expected_offset
            || mux[1].as_u32() as usize != expected_shift
            || mux[2].as_u32() != expected_mask
            || mux[3].as_u32() != 0
            || pin.get_prop_u32(PropName("starfive,pin-ioconfig")) != Some(0x0f)
            || pin
                .get_prop_u32(PropName("starfive,pin-gpio-dout"))
                .is_none_or(|value| value > 0x7f)
            || pin
                .get_prop_u32(PropName("starfive,pin-gpio-doen"))
                .is_none_or(|value| value > 0x3f)
            || pin
                .get_prop_u32(PropName("starfive,pin-gpio-din"))
                .is_some_and(|value| value > 63)
        {
            return Err("SDIO1 pin configuration does not match VisionFive 2 routing");
        }
        if seen_pins & (1 << pad) != 0 {
            return Err("SDIO1 pin group contains a duplicate pad");
        }
        seen_pins |= 1 << pad;
        configured += 1;
    }
    if configured != 6 || seen_pins != 0x1f80 {
        return Err("SDIO1 pin group must configure exactly six pins");
    }

    // Configure mux/pad state before releasing SDIO1 from reset.
    for pin in pin_group.children() {
        let pad = pin
            .get_prop_u32(PropName("starfive,pins"))
            .expect("validated SDIO1 pad") as usize;
        let mux = pin
            .get_prop(PropName("starfive,pinmux"))
            .expect("validated SDIO1 pinmux")
            .words();
        let mux_offset = mux[0].as_u32() as usize;
        let mux_shift = mux[1].as_u32();
        let mux_mask = mux[2].as_u32();
        let mux_value = mux[3].as_u32();
        set_pin_mux(
            pad,
            pin.get_prop_u32(PropName("starfive,pin-gpio-din")),
            pin.get_prop_u32(PropName("starfive,pin-gpio-dout"))
                .expect("validated SDIO1 DOUT"),
            pin.get_prop_u32(PropName("starfive,pin-gpio-doen"))
                .expect("validated SDIO1 DOEN"),
        );
        set_bits(SYS_GPIO, mux_offset, mux_mask, mux_value << mux_shift);
        set_pin_pad_config(pad, 0x0f);
    }

    gate_clock(92)?; // SDIO1 AHB
    set_bits(SYS_CRG, 94 * 4, 0x80ff_ffff, (1 << 31) | sd_div as u32);
    release_reset(65)?; // SDIO1 AHB

    let host = unsafe {
        crate::io::sd::dw_mmc::DwMmc::new(
            host_base as usize,
            input_clock_hz,
            host.get_prop_u32(PropName("fifo-depth")).unwrap_or(32) as u16,
            PlatformTimer::microseconds,
        )
    };
    let index = match crate::io::sd::register(host) {
        Ok(index) => index,
        Err(error) => {
            println!("JH7110 SD: card initialization error: {:?}", error);
            return Err("VisionFive 2 SD card initialization failed");
        }
    };
    let info = crate::io::sd::devices()
        .into_iter()
        .find(|device| device.index == index)
        .and_then(|device| device.info)
        .ok_or("VisionFive 2 SD card was not published")?;
    println!(
        "SD: sd{} {:?} {} blocks, {}-bit at {} Hz",
        index,
        info.kind,
        info.capacity_blocks,
        if info.bus_width == crate::io::sd::BusWidth::Four {
            4
        } else {
            1
        },
        info.clock_hz
    );
    Ok(())
}

fn pll2_rate(osc_hz: u64) -> Result<u64, &'static str> {
    let pd_fbdiv = read(SYS_SYSCON, 0x2c);
    let frac_postdiv = read(SYS_SYSCON, 0x30);
    let prediv_reg = read(SYS_SYSCON, 0x34);
    let pd = (pd_fbdiv >> 15) & 3;
    let fbdiv = (pd_fbdiv >> 17) & 0xfff;
    let frac = frac_postdiv & 0x00ff_ffff;
    let postdiv = (frac_postdiv >> 28) & 3;
    let prediv = prediv_reg & 0x3f;
    if (pd != 0 && pd != 3) || fbdiv == 0 || prediv == 0 {
        return Err("invalid JH7110 PLL2 configuration");
    }
    let numerator = osc_hz * fbdiv as u64
        + if pd == 0 {
            (osc_hz * frac as u64) >> 24
        } else {
            0
        };
    Ok(numerator / ((prediv as u64) << postdiv))
}

fn gate_clock(id: usize) -> Result<(), &'static str> {
    let offset = id.checked_mul(4).ok_or("invalid JH7110 clock ID")?;
    let after = set_bits(SYS_CRG, offset, 1 << 31, 1 << 31);
    if after & (1 << 31) == 0 {
        return Err("JH7110 SD clock gate did not enable");
    }
    Ok(())
}

fn release_reset(id: usize) -> Result<(), &'static str> {
    let bank = id / 32;
    let mask = 1u32 << (id % 32);
    set_bits(SYS_CRG, 0x2f8 + bank * 4, mask, 0);
    let deadline = PlatformTimer::microseconds().saturating_add(1_000);
    loop {
        if read(SYS_CRG, 0x308 + bank * 4) & mask != 0 {
            return Ok(());
        }
        if PlatformTimer::microseconds() >= deadline {
            return Err("JH7110 SD clock/reset release timed out");
        }
        core::hint::spin_loop();
    }
}

fn expected_pinmux(pin: usize) -> Option<(usize, usize, u32)> {
    match pin {
        7 => Some((0x2b0, 2, 0x1c)),
        8 => Some((0x2b0, 5, 0xe0)),
        9 => Some((0x2b0, 8, 0x700)),
        10 => Some((0x29c, 2, 0x1c)),
        11 => Some((0x29c, 5, 0xe0)),
        12 => Some((0x29c, 8, 0x700)),
        _ => None,
    }
}

fn set_pin_mux(pin: usize, din: Option<u32>, dout: u32, doen: u32) {
    let shift = (pin % 4) * 8;
    set_bits(
        SYS_GPIO,
        0x040 + 4 * (pin / 4),
        0x7f << shift,
        dout << shift,
    );
    set_bits(
        SYS_GPIO,
        0x000 + 4 * (pin / 4),
        0x3f << shift,
        doen << shift,
    );
    if let Some(din) = din {
        let input_shift = (din as usize % 4) * 8;
        let input_offset = 0x080 + 4 * (din as usize / 4);
        set_bits(
            SYS_GPIO,
            input_offset,
            0x7f << input_shift,
            ((pin as u32 + 2) & 0x7f) << input_shift,
        );
    }
}

fn set_pin_pad_config(pin: usize, value: u32) {
    if pin > 74 {
        return;
    }
    set_bits(
        SYS_GPIO,
        0x120 + 4 * pin,
        (3 << 1) | (1 << 3) | (1 << 6) | 1 | (1 << 5),
        value,
    );
}

fn read(base: u64, offset: usize) -> u32 {
    unsafe { ((base as usize + offset) as *const u32).read_volatile() }
}

fn set_bits(base: u64, offset: usize, mask: u32, value: u32) -> u32 {
    let pointer = (base as usize + offset) as *mut u32;
    let old = unsafe { pointer.read_volatile() };
    let new = (old & !mask) | (value & mask);
    unsafe { pointer.write_volatile(new) };
    unsafe { pointer.read_volatile() }
}

//! Raspberry Pi 3 USB runtime gate and DWC2 integration.

use alloc::boxed::Box;

use fdt::PropName;

use super::dwc2::{DmaMap, Dwc2, install_interrupt, interrupt_handler};
use super::{MachineType, current_machine_type, mbox};
use crate::arch::gic::Irq;
use crate::env::SystemService;
use crate::io::usb::UsbManager;
use crate::io::usb::hcd::HostController;
use crate::io::usb::input::UsbTextInputMux;
use crate::platform::arm64dt::{counter_us, dt, irq};
use crate::{System, usb_println};

const COMPATIBLE: &[&str] = &["brcm,bcm2708-usb", "brcm,bcm2835-usb"];

#[derive(Clone, Copy, Debug)]
pub struct Probe {
    pub mmio_base: usize,
    pub mmio_size: usize,
    pub irq: Irq,
    pub dma: DmaMap,
}

pub fn probe(tree: &fdt::DeviceTree) -> Option<Probe> {
    if current_machine_type() != MachineType::RaspberryPi3 {
        return None;
    }
    let (mmio_base, mmio_size, irq_number) = dt::find_map(tree, |node, map| {
        if !node.is_compatible_with_any(COMPATIBLE) {
            return None;
        }
        let (base, size) = map.reg(node, 0)?;
        let words = node.get_prop(PropName::INTERRUPTS)?.words();
        let irq = match words {
            [bank, bit, ..] => match bank.as_u32() {
                0 => 64 + bit.as_u32(),
                1 => bit.as_u32(),
                2 => 32 + bit.as_u32(),
                _ => return None,
            },
            [bit] => bit.as_u32(),
            _ => return None,
        };
        Some((base, size, irq))
    })?;
    let dma = dt::find_map(tree, |node, _| node.dma_ranges()?.next()).map(|r| DmaMap {
        cpu_start: r.parent,
        bus_start: r.child,
        length: r.len,
    })?;
    Some(Probe {
        mmio_base,
        mmio_size,
        irq: Irq(irq_number),
        dma,
    })
}

pub unsafe fn init(tree: &fdt::DeviceTree) -> Result<(), &'static str> {
    let probe = probe(tree).ok_or("unsupported or incomplete USB device-tree node")?;
    if probe.mmio_size < 0x1000 {
        return Err("DWC2 MMIO region is too small");
    }
    mbox::power_usb_hcd().map_err(|_| "firmware rejected USB power request")?;
    let controller = unsafe { Dwc2::new(probe.mmio_base, probe.dma, counter_us) }
        .map_err(|_| "DWC2 initialization failed")?;
    let cap = controller.capabilities();
    let phy = match cap.hs_phy_type {
        1 => "UTMI+",
        2 => "ULPI",
        3 => "UTMI+/ULPI",
        _ => "FS-only",
    };
    usb_println!(
        "USB DWC2: {:08x}, {} channels, FIFO {} words, PHY {}/{}",
        cap.snpsid,
        cap.host_channels,
        cap.fifo_depth_words,
        phy,
        cap.utmi_width
    );
    usb_println!(
        "USB GHWCFG: {:08x} {:08x} {:08x} {:08x} (descriptor DMA {})",
        cap.hwcfg[0],
        cap.hwcfg[1],
        cap.hwcfg[2],
        cap.hwcfg[3],
        if cap.descriptor_dma {
            "available, unused"
        } else {
            "absent"
        }
    );
    let regs = controller.register_snapshot();
    usb_println!(
        "USB regs: GUSBCFG={:08x} HCFG={:08x} HPRT={:08x} PCGCTL={:08x} GINTSTS={:08x}",
        regs.gusbcfg,
        regs.hcfg,
        regs.hprt,
        regs.pcgctl,
        regs.gintsts
    );
    usb_println!("USB root port: {:?}", controller.root_port_state());
    // Route completions through the GPU-interrupt cascade so the system can
    // sleep between transfers. The ISR only latches and acknowledges; the
    // manager still reaps from System::poll_services(). If the interrupt ever
    // turns out to be unserviceable the handler takes it back off the cascade
    // and the driver degrades to polling HCINT on its own; see stage 3 of
    // docs/USB_HOST_RPI3_PLAN.md for what that path cost to get right.
    install_interrupt(probe.mmio_base, probe.irq);
    unsafe { irq::register_usb_handler(probe.irq, interrupt_handler) };
    usb_println!("USB completion mode: interrupt (IRQ {})", probe.irq.0);
    let mut manager = UsbManager::new(Box::new(controller), counter_us);

    // Give initial enumeration a bounded foreground window while the UART is
    // still the active console.  Afterwards the same manager continues as a
    // background service, so an absent/slow keyboard never blocks boot.
    let bootstrap_start = counter_us();
    while !manager.keyboard_ready() && counter_us().wrapping_sub(bootstrap_start) < 3_000_000 {
        manager.poll().map_err(|_| "USB bootstrap polling failed")?;
        core::hint::spin_loop();
    }
    System::register_service(Box::new(manager)).map_err(|_| "USB service registration failed")?;
    let fallback = System::stdin();
    let mux = Box::leak(Box::new(UsbTextInputMux::new(fallback)));
    unsafe { System::set_stdin(mux) };
    Ok(())
}

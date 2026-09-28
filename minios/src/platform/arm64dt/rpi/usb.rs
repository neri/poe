//! Raspberry Pi 3 USB runtime gate and DWC2 integration.

use alloc::boxed::Box;

use fdt::PropName;

use super::armctrl::Armctrl;
use super::{MachineType, current_machine_type, mbox};
use crate::arch::cache;
use crate::arch::gic::Irq;
use crate::env::SystemService;
use crate::io::usb::UsbManager;
use crate::io::usb::dwc2::{DmaMap, Dwc2, Dwc2Platform, install_interrupt, interrupt_handler};
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
    let platform = Dwc2Platform {
        now_us: counter_us,
        dcache_clean: cache::dcache_clean,
        dcache_invalidate: cache::dcache_invalidate,
        dcache_clean_invalidate: cache::dcache_clean_invalidate,
        // Bit 4 is the BCM2835 wait-for-AXI-writes integration setting.
        ahb_config: 1 << 4,
        utmi_width: None,
        full_speed_only: false,
    };
    let controller = unsafe { Dwc2::new(probe.mmio_base, probe.dma, platform) }
        .map_err(|_| "DWC2 initialization failed")?;
    controller.log_state();
    // Route completions through the GPU-interrupt cascade so the system can
    // sleep between transfers. The ISR only latches and acknowledges; the
    // manager still reaps from System::poll_services(). If the interrupt ever
    // turns out to be unserviceable the handler takes it back off the cascade
    // and the driver degrades to polling HCINT on its own; see stage 3 of
    // docs/USB_HOST_RPI3_PLAN.md for what that path cost to get right.
    install_interrupt(probe.mmio_base, probe.irq.0, |irq| unsafe {
        Armctrl::disable(Irq(irq))
    });
    unsafe { irq::register_usb_handler(probe.irq, interrupt_handler) };
    usb_println!("USB completion mode: interrupt (IRQ {})", probe.irq.0);
    crate::io::usb::class::msc::registry::with_global(|r| r.set_clock(counter_us));
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

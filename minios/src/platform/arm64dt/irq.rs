//! Interrupt controller, selected from the device tree
//!
//! GICv3, GICv2, or the ARM local interrupt controller of BCM2836/BCM2837 (Raspberry Pi 2/3).

use super::dt::find_reg;
use super::rpi::armctrl::Armctrl;
use super::rpi::local_intc::LocalIntc;
use crate::arch::gic::{Gic, Irq};
use crate::arch::gicv3::GicV3;
use crate::*;

static mut IRQ_CONTROLLER: IrqController = IrqController::GicV2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IrqController {
    GicV2,
    GicV3,
    /// Raspberry Pi 2/3
    LocalIntc,
}

/// Initializes the interrupt controller found in the device tree:
/// GICv3, GICv2, then the local interrupt controller of Raspberry Pi 2/3.
///
/// Returns a description of the controller for the boot message.
pub unsafe fn init(dt: &fdt::DeviceTree) -> heapless::String<64> {
    const GICV2: &[&str] = &["arm,cortex-a15-gic", "arm,gic-400"];
    const GICV3: &[&str] = &["arm,gic-v3"];

    let mut info = heapless::String::new();
    unsafe {
        if let (Some((gicd_base, _)), Some((gicr_base, gicr_size))) =
            (find_reg(dt, GICV3, 0), find_reg(dt, GICV3, 1))
        {
            GicV3::init(gicd_base, gicr_base, gicr_size).expect("GICv3: initialization failed");
            IRQ_CONTROLLER = IrqController::GicV3;
            let _ = write!(
                info,
                "GICv3: GICD: {:08x} GICR: {:08x}",
                gicd_base, gicr_base
            );
        } else if let (Some((gicd_base, _)), Some((gicc_base, _))) =
            (find_reg(dt, GICV2, 0), find_reg(dt, GICV2, 1))
        {
            Gic::init(gicc_base, gicd_base);
            IRQ_CONTROLLER = IrqController::GicV2;
            let _ = write!(
                info,
                "GICv2: GICD: {:08x} GICC: {:08x}",
                gicd_base, gicc_base
            );
        } else if let Some((base, _)) = find_reg(dt, &[LocalIntc::COMPATIBLE], 0) {
            LocalIntc::init(base);
            if let Some((armctrl, _)) = find_reg(dt, &[Armctrl::COMPATIBLE], 0) {
                Armctrl::init(armctrl)
            }
            IRQ_CONTROLLER = IrqController::LocalIntc;
            let _ = write!(info, "Local interrupt controller: {:08x}", base);
        } else {
            panic!("interrupt controller not found");
        }
    }
    info
}

static mut HANDLERS: [Option<(Irq, fn())>; 64] = [None; 64];

/// Registers an interrupt handler before CPU interrupts are enabled.
/// The handler must acknowledge its device source before returning.
pub unsafe fn register_handler(irq: Irq, handler: fn()) -> Result<(), &'static str> {
    unsafe { register_handler_inner(irq, handler, None) }
}

pub unsafe fn register_handler_trigger(
    irq: Irq,
    handler: fn(),
    edge: bool,
) -> Result<(), &'static str> {
    unsafe { register_handler_inner(irq, handler, Some(edge)) }
}

unsafe fn register_handler_inner(
    irq: Irq,
    handler: fn(),
    trigger: Option<bool>,
) -> Result<(), &'static str> {
    if !can_enable(irq) {
        return Err("unsupported IRQ");
    }
    unsafe {
        let _guard = Hal::cpu().interrupt_guard();
        let handlers = &mut *(&raw mut HANDLERS);
        if handlers
            .iter()
            .flatten()
            .any(|(registered, _)| *registered == irq)
        {
            return Err("IRQ already registered");
        }
        let slot = handlers
            .iter_mut()
            .find(|slot| slot.is_none())
            .ok_or("IRQ registry full")?;
        if irq.0 >= 32
            && let Some(edge) = trigger
        {
            match IRQ_CONTROLLER {
                IrqController::GicV2 => Gic::configure_spi(irq, edge),
                IrqController::GicV3 => GicV3::configure_spi(irq, edge),
                IrqController::LocalIntc => {}
            }
        }
        *slot = Some((irq, handler));
        enable(irq);
    }
    Ok(())
}

#[cfg(feature = "usb")]
pub unsafe fn register_usb_handler(irq: Irq, handler: fn()) {
    unsafe { register_handler(irq, handler).expect("USB IRQ registration failed") }
}

pub fn dispatch(irq: Irq) -> bool {
    unsafe {
        for &(registered, handler) in (&raw const HANDLERS).as_ref().unwrap().iter().flatten() {
            if irq == registered {
                handler();
                return true;
            }
        }
    }
    false
}

/// True if the interrupt controller in use can enable `irq`.
///
/// A driver that finds an unsupported interrupt in the device tree can fall
/// back to polling instead of stopping the boot.
pub fn can_enable(irq: Irq) -> bool {
    unsafe {
        match IRQ_CONTROLLER {
            IrqController::GicV3 => GicV3::can_enable(irq),
            IrqController::GicV2 => Gic::can_enable(irq),
            IrqController::LocalIntc => true,
        }
    }
}

pub unsafe fn enable(irq: Irq) {
    unsafe {
        match IRQ_CONTROLLER {
            IrqController::GicV2 => Gic::enable(irq),
            IrqController::GicV3 => GicV3::enable(irq),
            IrqController::LocalIntc => LocalIntc::enable(irq),
        }
    }
}

/// Acknowledges the pending interrupt. An ID of 1020 or greater means no interrupt.
pub unsafe fn ack() -> Irq {
    unsafe {
        match IRQ_CONTROLLER {
            IrqController::GicV2 => Gic::ack(),
            IrqController::GicV3 => GicV3::ack(),
            IrqController::LocalIntc => LocalIntc::ack(),
        }
    }
}

pub unsafe fn eoi(irq: Irq) {
    unsafe {
        match IRQ_CONTROLLER {
            IrqController::GicV2 => Gic::eoi(irq),
            IrqController::GicV3 => GicV3::eoi(irq),
            IrqController::LocalIntc => LocalIntc::eoi(irq),
        }
    }
}

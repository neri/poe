//! Interrupt controller, selected from the device tree
//!
//! GICv3, GICv2, or the ARM local interrupt controller of BCM2836/BCM2837 (Raspberry Pi 2/3).

use super::dt::find_reg;
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
            IRQ_CONTROLLER = IrqController::LocalIntc;
            let _ = write!(info, "Local interrupt controller: {:08x}", base);
        } else {
            panic!("interrupt controller not found");
        }
    }
    info
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

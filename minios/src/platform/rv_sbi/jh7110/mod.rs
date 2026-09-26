//! StarFive JH7110 (VisionFive 2) board support.
//!
//! PCIe0 carries a VL805 xHCI controller and HDMI is driven by the DC8200
//! display controller. Both are brought up here without relying on U-Boot.

use fdt::DeviceTree;

use crate::{System, println};

mod hdmi;
mod pcie;

/// Returns whether the device tree describes a VisionFive 2 board.
pub(super) fn is_visionfive2(tree: &DeviceTree) -> bool {
    tree.root().is_compatible_with("starfive,visionfive-v2")
}

/// Brings up the PCIe root ports and verifies the VL805 during early
/// device-tree init. Bus mastering stays disabled until [`init`].
pub(super) fn init_early(tree: &DeviceTree) {
    if tree.root().is_compatible_with("starfive,jh7110") {
        pcie::report(tree);
    }
}

/// Starts USB and HDMI on a VisionFive 2.
pub(super) fn init(tree: &DeviceTree) {
    if !is_visionfive2(tree) {
        return;
    }

    #[cfg(feature = "usb")]
    if let Err(reason) = init_usb() {
        println!("JH7110 USB disabled: {}", reason);
    }

    if let Err(reason) = hdmi::init(tree) {
        println!("JH7110 HDMI disabled: {}", reason);
    }
}

#[cfg(feature = "usb")]
fn init_usb() -> Result<(), &'static str> {
    use crate::io::pci::host::DmaWindow;

    let bar = pcie::prepare_vl805_dma()?;
    let info = System::boot_info();
    let start = info.start_conventional_memory as u64;
    let end = start + info.conventional_memory_size as u64;
    let result = super::usb::start_xhci(
        bar,
        DmaWindow {
            cpu_start: start,
            cpu_end: end,
            offset: 0,
        },
    );
    if result.is_err() {
        pcie::disable_vl805_dma();
    }
    result
}

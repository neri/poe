//! Chromebooks: coreboot table, VPD and ChromeOS EC
//!
//! The EC is reached through [`SpiDevice`](super::spi::SpiDevice), provided by the SoC's SPI driver.

use cros_ec::CrosEc;

use super::spi::SpiDevice;
use crate::*;

pub mod coreboot;
pub mod cros_ec;
pub mod cros_ec_keyb;
pub mod ec_packet;
pub mod keymatrix;
pub mod vpd;

/// Uses the keyboard scanned by the EC as stdin, in preference to UART.
///
/// The layout is decided from the VPD, the same way as ChromeOS.
pub unsafe fn install_keyboard<S: SpiDevice + 'static>(dt: &fdt::DeviceTree, ec: CrosEc<S>) {
    let vpd = coreboot::find_vpd_ro(dt);
    let layout = vpd
        .as_deref()
        .map_or(vpd::KeyboardLayout::Us, vpd::keyboard_layout);
    println!(
        "Keyboard layout: {:?} (VPD {})",
        layout,
        if vpd.is_some() { "found" } else { "not found" }
    );

    if unsafe { cros_ec_keyb::CrosEcKeyboard::install(ec, layout) } {
        println!("Keyboard: ChromeOS EC");
    }
}

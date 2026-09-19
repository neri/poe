//! PSCI (Power State Coordination Interface)

use core::arch::asm;

use fdt::PropName;

use super::dt;

const PSCI_SYSTEM_RESET: u32 = 0x8400_0009;

static mut CONDUIT: Conduit = Conduit::None;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Conduit {
    /// PSCI is not available (e.g. Raspberry Pi)
    None,
    Hvc,
    Smc,
}

/// Finds the conduit (`hvc` or `smc`) in the device tree.
pub unsafe fn init(dt: &fdt::DeviceTree) {
    let conduit = dt::find_map(dt, |node, _| {
        node.is_compatible_with("arm,psci-0.2").then(|| {
            match node.get_prop_str(PropName::new("method")) {
                Some("hvc") => Conduit::Hvc,
                Some("smc") => Conduit::Smc,
                _ => Conduit::None,
            }
        })
    });
    unsafe {
        CONDUIT = conduit.unwrap_or(Conduit::None);
    }
}

/// Resets the system. Returns if PSCI is not available.
pub unsafe fn system_reset() {
    unsafe { call(PSCI_SYSTEM_RESET) }
}

unsafe fn call(function_id: u32) {
    unsafe {
        match CONDUIT {
            Conduit::Hvc => {
                asm!("hvc #0", inlateout("x0") function_id as usize => _, clobber_abi("C"))
            }
            Conduit::Smc => {
                asm!("smc #0", inlateout("x0") function_id as usize => _, clobber_abi("C"))
            }
            Conduit::None => {}
        }
    }
}

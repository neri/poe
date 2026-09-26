//! Arm device-tree PCI host compatibility exports.
//!
//! Host bridge parsing and BAR allocation are shared with RISC-V. The Arm
//! xHCI attachment maps raw interrupt numbers into its GIC IRQ type.

pub use crate::io::pci::Bdf;
pub use crate::io::pci::host::*;

//! Basic Input/Output System (BIOS) services for NEC PC-9801 series

use x86::prot::InterruptVector;

/// Video and keyboard BIOS Services
pub const INT18: InterruptVector = InterruptVector(0x18);

/// Disk BIOS Services
pub const INT1B: InterruptVector = InterruptVector(0x1B);

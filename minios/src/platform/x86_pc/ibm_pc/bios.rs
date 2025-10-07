//! Basic Input/Output System services for IBM PC architecture

use x86::prot::InterruptVector;

/// Video BIOS Services
pub const INT10: InterruptVector = InterruptVector(0x10);

/// Disk BIOS Services
pub const INT13: InterruptVector = InterruptVector(0x13);

/// Misc BIOS Services
pub const INT15: InterruptVector = InterruptVector(0x15);

/// Keyboard BIOS Services
pub const INT16: InterruptVector = InterruptVector(0x16);

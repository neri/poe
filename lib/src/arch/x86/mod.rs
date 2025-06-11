//! Arch for x86

pub(in crate::arch) mod bits;
pub mod cpu;
pub(crate) mod lomem;
pub(in crate::arch) mod setjmp;
pub mod vm86;

mod hal_x86;
pub use hal_x86::*;

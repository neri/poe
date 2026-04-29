//! Architecture dependent

pub mod hal;
pub mod spinlock;

#[cfg(target_arch = "x86")]
mod x86;
#[cfg(target_arch = "x86")]
#[allow(unused_imports)]
pub use x86::*;

#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(target_arch = "aarch64")]
#[allow(unused_imports)]
pub use aarch64::*;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[allow(unused_imports)]
pub use riscv::*;

//! basic spinlock implementation for different architectures

#[cfg(target_arch = "aarch64")]
mod aa64;
#[cfg(target_arch = "aarch64")]
pub use aa64::*;

#[cfg(target_arch = "riscv64")]
mod spinlock;
#[cfg(target_arch = "riscv64")]
pub use spinlock::*;

#[cfg(target_arch = "x86")]
mod x86;
#[cfg(target_arch = "x86")]
pub use x86::*;

#[cfg(target_arch = "x86_64")]
mod x86_64;
#[cfg(target_arch = "x86_64")]
pub use x86_64::*;

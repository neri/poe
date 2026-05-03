//! RISCV CPU
#![cfg_attr(not(test), no_std)]

pub mod csr;

#[cfg(target_arch = "riscv32")]
pub const XLEN: usize = 32;
#[cfg(target_arch = "riscv64")]
pub const XLEN: usize = 64;

pub const XLEN_BYTES: usize = XLEN / 8;

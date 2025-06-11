//! RISCV CPU

pub const XLEN_BYTES: usize = core::mem::size_of::<usize>();
pub const XLEN: usize = XLEN_BYTES * 8;

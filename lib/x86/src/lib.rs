//! My x86 libraries
#![cfg_attr(not(test), no_std)]

extern crate alloc;
pub mod cpuid;
pub mod cr;
pub mod efer;
pub mod gpr;
pub mod isolated_io;
pub mod msr;
pub mod prot;
pub mod real;

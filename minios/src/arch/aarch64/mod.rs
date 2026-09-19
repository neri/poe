//! Arch for arm64 (aarch64)

mod hal_aa64;
#[allow(unused_imports)]
pub use hal_aa64::*;

pub mod cache;
pub mod gic;
pub mod gicv3;
pub mod timer;

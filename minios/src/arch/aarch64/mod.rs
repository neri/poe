//! Arch for arm64 (aarch64)

mod hal_aa64;
#[allow(unused_imports)]
pub use hal_aa64::*;

pub mod gic;
pub mod timer;

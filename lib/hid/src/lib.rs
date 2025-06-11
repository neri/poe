//! Human Interface Device

#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod hid;
pub use hid::*;

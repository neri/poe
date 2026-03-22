//! Human Interface Devices

#![cfg_attr(not(test), no_std)]
#![feature(iter_advance_by)]

extern crate alloc;

mod hid;
pub use hid::*;

#[path = "layouts/mod.rs"]
pub mod layouts;

pub mod parser;

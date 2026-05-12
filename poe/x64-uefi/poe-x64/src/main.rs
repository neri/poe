//! Pre-OS Execution Environment for x64
#![no_std]
#![no_main]

use poe::prelude::*;
use uefi::prelude::*;

#[entry]
fn main() -> Status {
    unsafe { System::init_uefi(0, poe::main) }
}

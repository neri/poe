//! Pre-OS Execution Environment for x86
#![no_std]
#![no_main]
use poe::prelude::*;

#[unsafe(no_mangle)]
extern "fastcall" fn _start(info: &BootInfo) -> ! {
    unsafe {
        System::init(info, 0, poe::main);
    }
}

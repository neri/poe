// MEG-OS Kernel
// Copyright (c) 2021 MEG-OS project

#![no_std]
#![no_main]
#![feature(asm)]

use kernel::*;

entry!(Application::main);

#[used]
static mut MAIN: Application = Application::new();

struct Application {}

impl Application {
    const fn new() -> Self {
        Self {}
    }

    fn main() {
        loop {
            kernel_halt();
        }
    }
}

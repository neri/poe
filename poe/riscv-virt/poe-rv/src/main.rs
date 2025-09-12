//! Pre-OS Execution Environment for riscv-virtio
#![no_std]
#![no_main]

use core::{arch::naked_asm, ffi::c_void};
use poe::prelude::*;

unsafe extern "C" {
    unsafe static __bss: c_void;
    unsafe static __ebss: c_void;
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.boot")]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "la sp, __stack_top",
        "j {start}",
        start = sym _arch_riscv_start,
    )
}

pub unsafe extern "C" fn _arch_riscv_start(hart_id: usize, dtb: usize) -> ! {
    unsafe {
        let bss = &__bss as *const _ as *mut u8;
        let ebss = &__ebss as *const _;
        bss.write_bytes(0, ebss as usize - bss as usize);

        System::init_dt(dtb, hart_id, poe::main);
    }
}

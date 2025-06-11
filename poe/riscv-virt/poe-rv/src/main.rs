//! Pre-OS Execution Environment for riscv
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
        "
            j 1f
            .balign 8
            // Linux Kernel Image Header
            .dword __kernel_base - 0x80000000   /* Image load offset, little endian */
            .dword __edata - __kernel_base      /* Effective Image size, little endian */
            .dword 0            /* kernel flags, little endian */
            .word 0x00000002    /* Version of this header */
            .word 0             /* Reserved */
            .dword 0            /* Reserved */
            .dword 0x5643534952 /* Magic number, little endian, \"RISCV\" */
            .word 0x05435352    /* Magic number 2, little endian, \"RSC\x05\" */
            .word 0             /* Reserved for PE COFF offset */
        1:
            la sp, __stack_top
            j {start}
            ",
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

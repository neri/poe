//! Pre-OS Execution Environment for Raspberry Pi
#![no_std]
#![no_main]

use core::arch::naked_asm;
use poe::prelude::*;

fn _arch_rpi_start(dtb: usize) -> ! {
    unsafe { System::init_dt(dtb, 0, poe::main) }
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.boot")]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "    mrs     x1, mpidr_el1",
        "    and     x1, x1, #3",
        "    cbz     x1, 102f",
        "",
        "    mov     x2, #0xd8",
        "100:",
        "    ldr     x3, [x2, x1, lsl #3]",
        "    cbnz    x3, 101f",
        "    wfe",
        "    b       100b",
        "101:",
        "    lsl     x4, x1, #16",
        "    add     x4, x4, #0x10000",
        "    b       103f",
        "",
        "102:",
        "    adr     x4, _start",
        "103:",
        "    mov     sp, x4",
        "    msr     sp_el1, x4",
        "",
        "    mov     x2, #3 << 20",
        "    msr     cpacr_el1, x2",
        "",
        "    mrs     x2, currentel",
        "    and     x2, x2, #0xC",
        "    cmp     x2, #0x4",
        "    b.eq    104f",
        "",
        "    mrs     x2, midr_el1",
        "    mrs     x3, mpidr_el1",
        "    msr     vpidr_el2, x2",
        "    msr     vmpidr_el2, x3",
        "",
        "    mov     x2, #0x0002",
        "    movk    x2, #0x8000, lsl #16",
        "    msr     hcr_el2, x2",
        "    adr     x3, 104f",
        "    msr     elr_el2, x3",
        "    mov     x4, #0x03C5",
        "    msr     spsr_el2, x4",
        "    eret",
        "104:",
        "",
        "    mrs     x1, mpidr_el1",
        "    and     x1, x1, #3",
        "    cbz     x1, 2f",
        "",
        "105:",
        "    wfe",
        "    b       105b",
        "",
        "2:",
        "    ldr     x1, =__bss_start",
        "    ldr     w2, =__bss_size",
        "3:  cbz     w2, 4f",
        "    str     xzr, [x1], #8",
        "    sub     w2, w2, #1",
        "    cbnz    w2, 3b",
        "",
        "4:  bl      {main}",
        "5:",
        main = sym _arch_rpi_start,
    )
}

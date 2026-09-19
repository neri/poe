//! Pre-OS Execution Environment for rv64-virt
#![no_std]
#![no_main]

use core::arch::naked_asm;

use poe::prelude::*;

/// The image is linked at 0 as a PIE and may be loaded at any 4KB aligned address.
/// Until the relocations are applied, only PC-relative addressing (`lla`) may be used.
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.head")]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        // Linux RISC-V Image header (Documentation/arch/riscv/boot-image-header.rst)
        ".option push",
        ".option norvc",
        "    j       1f",                  // code0
        "    .long   0",                   // code1
        "    .quad   0x200000",            // text_offset
        "    .quad   __image_size",        // image_size
        "    .quad   0",                   // flags: little endian
        "    .long   2",                   // version 0.2
        "    .long   0",                   // res1
        "    .quad   0",                   // res2
        "    .ascii  \"RISCV\\0\\0\\0\"",  // magic (deprecated)
        "    .ascii  \"RSC\\x05\"",        // magic2
        "    .long   0",                   // res3
        ".option pop",
        "",
        "1:",
        // Apply R_RISCV_RELATIVE relocations (t0 = load address = delta)
        "    lla     t0, _start",
        "    lla     t1, __rela_start",
        "    lla     t2, __rela_end",
        "2:  bgeu    t1, t2, 3f",
        "    ld      t3, 0(t1)",           // r_offset
        "    ld      t4, 8(t1)",           // r_info
        "    ld      t5, 16(t1)",          // r_addend
        "    addi    t1, t1, 24",
        "    li      t6, 3",               // R_RISCV_RELATIVE
        "    bne     t4, t6, 9f",
        "    add     t5, t5, t0",
        "    add     t3, t3, t0",
        "    sd      t5, 0(t3)",
        "    j       2b",
        "",
        "3:  lla     t1, __bss_start",
        "    lla     t2, __bss_end",
        "4:  bgeu    t1, t2, 5f",
        "    sd      zero, 0(t1)",
        "    addi    t1, t1, 8",
        "    j       4b",
        "",
        "5:  lla     sp, __stack_top",
        "    j       {main}",
        "",
        "9:  wfi",
        "    j       9b",
        main = sym _arch_riscv_start,
    )
}

pub unsafe extern "C" fn _arch_riscv_start(hart_id: usize, dtb: usize) -> ! {
    unsafe {
        System::init_dt(dtb, hart_id, poe::main);
    }
}

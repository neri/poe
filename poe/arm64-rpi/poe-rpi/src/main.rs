//! Pre-OS Execution Environment for Raspberry Pi
#![no_std]
#![no_main]

use core::arch::naked_asm;
use poe::prelude::*;

fn rpi_main(dtb: usize) -> ! {
    unsafe {
        System::init_dt(dtb, 0, poe::main);
    }
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.boot")]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "
        mrs     x1, mpidr_el1
        and     x1, x1, #3
        cbz     x1, 102f

        mov     x2, #0xd8
    100:
        ldr     x3, [x2, x1, lsl #3]
        cbnz    x3, 101f
        wfe
        b       100b
    101:
        lsl     x4, x1, #16
        add     x4, x4, #0x10000
        b       103f

    102:
        adr     x4, _start
    103:
        mov     sp, x4
        msr     sp_el1, x4

        mov     x2, #3 << 20
        msr     cpacr_el1, x2

        mrs     x2, currentel
        and     x2, x2, #0xC
        cmp     x2, #0x4
        b.eq    104f

        mrs     x2, midr_el1
        mrs     x3, mpidr_el1
        msr     vpidr_el2, x2
        msr     vmpidr_el2, x3

        mov     x2, #0x0002
        movk    x2, #0x8000, lsl #16
        msr     hcr_el2, x2
        adr     x3, 104f
        msr     elr_el2, x3
        mov     x4, #0x03C5
        msr     spsr_el2, x4
        eret
    104:

        ldr     x1, =_vector_table
        msr     vbar_el1, x1

        mrs     x1, mpidr_el1
        and     x1, x1, #3
        cbz     x1, 2f

    105:
        wfe
        b       105b

    2:
        ldr     x1, =__bss_start
        ldr     w2, =__bss_size
    3:  cbz     w2, 4f
        str     xzr, [x1], #8
        sub     w2, w2, #1
        cbnz    w2, 3b

    4:  bl      {main}
    5:
    ",
        main = sym rpi_main,
    )
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.boot")]
#[allow(named_asm_labels)]
unsafe extern "C" fn _vector_table_nkf() {
    naked_asm!(
        "
    .align 11
    _vector_table:
    // synchronous
        sub sp, sp, #256
        stp x0, x1, [sp, #16 * 0]
        stp x2, x3, [sp, #16 * 1]
        stp x4, x5, [sp, #16 * 2]
        stp x6, x7, [sp, #16 * 3]
        stp x8, x9, [sp, #16 * 4]
        stp x10, x11, [sp, #16 * 5]
        stp x12, x13, [sp, #16 * 6]
        stp x14, x15, [sp, #16 * 7]
        stp x16, x17, [sp, #16 * 8]
        stp x18, x19, [sp, #16 * 9]
        stp x20, x21, [sp, #16 * 10]
        stp x22, x23, [sp, #16 * 11]
        stp x24, x25, [sp, #16 * 12]
        stp x26, x27, [sp, #16 * 13]
        stp x28, x29, [sp, #16 * 14]
        str x30, [sp, #16 * 15]
        brk #1
    .align 7
    // IRQ
    brk #1

    .align 7
    // FIQ
    brk #1

    .align 7
    // SError
    brk #1

    .align 7
    _exc_handler:
            brk #1
            ldp x0, x1, [sp, #16 * 0]
            ldp x2, x3, [sp, #16 * 1]
            ldp x4, x5, [sp, #16 * 2]
            ldp x6, x7, [sp, #16 * 3]
            ldp x8, x9, [sp, #16 * 4]
            ldp x10, x11, [sp, #16 * 5]
            ldp x12, x13, [sp, #16 * 6]
            ldp x14, x15, [sp, #16 * 7]
            ldp x16, x17, [sp, #16 * 8]
            ldp x18, x19, [sp, #16 * 9]
            ldp x20, x21, [sp, #16 * 10]
            ldp x22, x23, [sp, #16 * 11]
            ldp x24, x25, [sp, #16 * 12]
            ldp x26, x27, [sp, #16 * 13]
            ldp x28, x29, [sp, #16 * 14]
            ldr x30, [sp, #16 * 15] 
            add sp, sp, #256
            eret",
    );
}

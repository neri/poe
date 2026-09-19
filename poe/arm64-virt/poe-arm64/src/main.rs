//! Pre-OS Execution Environment for Arm virtual machine
#![no_std]
#![no_main]

use core::arch::naked_asm;

use poe::prelude::*;

fn _arch_virt_start(dtb: usize) -> ! {
    unsafe {
        minios::platform::virt::clean_dtb_cache(dtb);
    }
    #[cfg(feature = "diag")]
    unsafe {
        minios::platform::virt::diag::mark(0);
        minios::platform::virt::diag::check_dtb(dtb);
    }
    unsafe { System::init_dt(dtb, 0, poe::main) }
}

/// Entry point, placed after the Linux arm64 Image header.
///
/// The boot loader (QEMU `-kernel`, U-Boot `booti`, depthcharge, ...) passes
/// the physical address of the device tree blob in x0.
///
/// The image is linked at 0 as a PIE and may be loaded at any 4KB aligned address.
/// Until the relocations are applied, only PC-relative addressing may be used.
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.head")]
unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        // Linux arm64 Image header (Documentation/arch/arm64/booting.rst)
        "    b       1f",              // code0
        "    .long   0",               // code1
        "    .quad   0x80000",         // text_offset
        "    .quad   __image_size",    // image_size
        "    .quad   0b1000",          // flags: placement anywhere
        "    .quad   0, 0, 0",         // res2, res3, res4
        "    .long   0x644d5241",      // magic "ARM\x64"
        "    .long   0",               // res5
        "",
        "1:",
        "    mrs     x1, mpidr_el1",
        "    and     x1, x1, #0xff",
        "    cbnz    x1, 9f",
        "",
        // Apply R_AARCH64_RELATIVE relocations (x9 = load address = delta)
        "    adr     x9, _start",
        "    adrp    x10, __rela_start",
        "    add     x10, x10, :lo12:__rela_start",
        "    adrp    x11, __rela_end",
        "    add     x11, x11, :lo12:__rela_end",
        "5:  cmp     x10, x11",
        "    b.hs    6f",
        "    ldp     x12, x13, [x10], #16", // r_offset, r_info
        "    ldr     x14, [x10], #8",       // r_addend
        "    cmp     x13, #1027",           // R_AARCH64_RELATIVE
        "    b.ne    9f",
        "    add     x14, x14, x9",
        "    str     x14, [x12, x9]",
        "    b       5b",
        "",
        "6:  adrp    x1, __stack_top",
        "    add     x1, x1, :lo12:__stack_top",
        "    mov     sp, x1",
        "",
        "    mov     x2, #3 << 20",
        "    msr     cpacr_el1, x2",
        "",
        // SCTLR_EL1: MMU and caches off, little endian (its reset value is partly UNKNOWN)
        "    mov     x2, #0x0800",
        "    movk    x2, #0x30d0, lsl #16",
        "    msr     sctlr_el1, x2",
        "    isb",
        "",
        "    mrs     x2, currentel",
        "    and     x2, x2, #0xC",
        "    cmp     x2, #0x4",
        "    b.eq    2f",
        "",
        // Entered at EL2: drop to EL1
        "    msr     sp_el1, x1",
        "",
        // Do not trap FP/SIMD (TFP) and CP15 accesses of EL1 to EL2
        "    mov     x2, #0x33ff",
        "    msr     cptr_el2, x2",
        "    msr     hstr_el2, xzr",
        "",
        "    mrs     x2, cnthctl_el2",
        "    orr     x2, x2, #3",
        "    msr     cnthctl_el2, x2",
        "    msr     cntvoff_el2, xzr",
        "",
        "    mrs     x2, midr_el1",
        "    mrs     x3, mpidr_el1",
        "    msr     vpidr_el2, x2",
        "    msr     vmpidr_el2, x3",
        "",
        // Allow EL1 to use the GICv3 system registers, if implemented
        "    mrs     x2, id_aa64pfr0_el1",
        "    ubfx    x2, x2, #24, #4",
        "    cbz     x2, 7f",
        "    mrs     x2, icc_sre_el2",
        "    orr     x2, x2, #0xf", // SRE | DFB | DIB | Enable
        "    msr     icc_sre_el2, x2",
        "    isb",
        "    msr     ich_hcr_el2, xzr",
        "7:",
        "",
        "    mov     x2, #0x0002",
        "    movk    x2, #0x8000, lsl #16",
        "    msr     hcr_el2, x2",
        "    adr     x3, 2f",
        "    msr     elr_el2, x3",
        "    mov     x4, #0x03C5",
        "    msr     spsr_el2, x4",
        "    eret",
        "",
        "2:",
        "    adrp    x1, __bss_start",
        "    add     x1, x1, :lo12:__bss_start",
        "    adrp    x2, __bss_end",
        "    add     x2, x2, :lo12:__bss_end",
        "3:  cmp     x1, x2",
        "    b.hs    4f",
        "    str     xzr, [x1], #8",
        "    b       3b",
        "",
        "4:  bl      {main}",
        "",
        "9:  wfe",
        "    b       9b",
        main = sym _arch_virt_start,
    )
}

//! Platform dependent module for RISC-V virt machine

use super::*;
use crate::arch::csr::{CSR, VectorMode};
use crate::*;
use core::{arch::naked_asm, ffi::c_void, time::Duration};
use riscv::XLEN_BYTES;

mod sbi_console;
pub mod timer;

unsafe extern "C" {
    unsafe static _end: c_void;
}

impl PlatformTrait for Platform {
    unsafe fn init_dt_early(dt: &fdt::DeviceTree, arg: usize) {
        let hart_id = arg;
        unsafe {
            #[cfg(feature = "minisbi")]
            {
                minisbi::init();
            }

            sbi_console::SbiConsole::init();
            System::set_stdin(sbi_console::SbiConsole::shared());
            System::set_stdout(sbi_console::SbiConsole::shared());
            System::set_stderr(sbi_console::SbiConsole::shared());

            println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");

            let spec_ver = sbi::base::get_spec_version();
            let impl_id = sbi::base::get_impl_id().unwrap();
            let impl_ver = sbi::base::get_impl_version().unwrap();
            println!(
                "SBI version {}.{} impl {:?} version {:x}",
                spec_ver.major(),
                spec_ver.minor(),
                impl_id,
                impl_ver
            );

            println!("Hart ID: {}", hart_id);

            let boot_info = System::boot_info_mut();
            boot_info.platform = Platform::Virt;

            let end = PhysicalAddress::new(&_end as *const _ as PhysicalAddressRepr);
            boot_info.start_conventional_memory = end.rounding_up_4k().as_repr() as u32;
            boot_info.conventional_memory_size = 0x40_0000;

            println!("Model: {}", dt.root().model());
            for item in dt.root().compatible().unwrap() {
                println!("compatible: {}", item);
            }

            CSR::set_stvec(VectorMode::Direct, _arch_stvec as *const () as usize);

            timer::PlatformTimer::init(dt);
        }
    }

    unsafe fn init(_arg: usize) {
        unsafe {
            println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");

            Hal::cpu().enable_interrupt();
        }
    }

    unsafe fn exit() {
        // Nothing to do
    }

    fn reset_system() -> ! {
        sbi::legacy::shutdown();
    }

    fn halt() -> ! {
        sbi::legacy::shutdown()
    }

    #[inline]
    fn monotonic() -> u64 {
        timer::PlatformTimer::monotonic()
    }

    #[inline]
    fn duration_to_ticks(duration: Duration) -> u64 {
        timer::PlatformTimer::duration_to_ticks(duration)
    }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn _arch_stvec() -> ! {
    naked_asm!(
        "csrw sscratch, sp",
        "",
        "addi sp, sp, -{XLEN_BYTES} * 31",
        "sw ra,  {XLEN_BYTES} * 0(sp)",
        "sw gp,  {XLEN_BYTES} * 1(sp)",
        "sw tp,  {XLEN_BYTES} * 2(sp)",
        "sw t0,  {XLEN_BYTES} * 3(sp)",
        "sw t1,  {XLEN_BYTES} * 4(sp)",
        "sw t2,  {XLEN_BYTES} * 5(sp)",
        "sw t3,  {XLEN_BYTES} * 6(sp)",
        "sw t4,  {XLEN_BYTES} * 7(sp)",
        "sw t5,  {XLEN_BYTES} * 8(sp)",
        "sw t6,  {XLEN_BYTES} * 9(sp)",
        "sw a0,  {XLEN_BYTES} * 10(sp)",
        "sw a1,  {XLEN_BYTES} * 11(sp)",
        "sw a2,  {XLEN_BYTES} * 12(sp)",
        "sw a3,  {XLEN_BYTES} * 13(sp)",
        "sw a4,  {XLEN_BYTES} * 14(sp)",
        "sw a5,  {XLEN_BYTES} * 15(sp)",
        "sw a6,  {XLEN_BYTES} * 16(sp)",
        "sw a7,  {XLEN_BYTES} * 17(sp)",
        "sw s0,  {XLEN_BYTES} * 18(sp)",
        "sw s1,  {XLEN_BYTES} * 19(sp)",
        "sw s2,  {XLEN_BYTES} * 20(sp)",
        "sw s3,  {XLEN_BYTES} * 21(sp)",
        "sw s4,  {XLEN_BYTES} * 22(sp)",
        "sw s5,  {XLEN_BYTES} * 23(sp)",
        "sw s6,  {XLEN_BYTES} * 24(sp)",
        "sw s7,  {XLEN_BYTES} * 25(sp)",
        "sw s8,  {XLEN_BYTES} * 26(sp)",
        "sw s9,  {XLEN_BYTES} * 27(sp)",
        "sw s10, {XLEN_BYTES} * 28(sp)",
        "sw s11, {XLEN_BYTES} * 29(sp)",
        "",
        "csrr a0, sscratch",
        "sw a0, {XLEN_BYTES} * 30(sp)",
        "",
        "mv a0, sp",
        "call {arch_handle_trap}",
        "",
        "lw ra,  {XLEN_BYTES} * 0(sp)",
        "lw gp,  {XLEN_BYTES} * 1(sp)",
        "lw tp,  {XLEN_BYTES} * 2(sp)",
        "lw t0,  {XLEN_BYTES} * 3(sp)",
        "lw t1,  {XLEN_BYTES} * 4(sp)",
        "lw t2,  {XLEN_BYTES} * 5(sp)",
        "lw t3,  {XLEN_BYTES} * 6(sp)",
        "lw t4,  {XLEN_BYTES} * 7(sp)",
        "lw t5,  {XLEN_BYTES} * 8(sp)",
        "lw t6,  {XLEN_BYTES} * 9(sp)",
        "lw a0,  {XLEN_BYTES} * 10(sp)",
        "lw a1,  {XLEN_BYTES} * 11(sp)",
        "lw a2,  {XLEN_BYTES} * 12(sp)",
        "lw a3,  {XLEN_BYTES} * 13(sp)",
        "lw a4,  {XLEN_BYTES} * 14(sp)",
        "lw a5,  {XLEN_BYTES} * 15(sp)",
        "lw a6,  {XLEN_BYTES} * 16(sp)",
        "lw a7,  {XLEN_BYTES} * 17(sp)",
        "lw s0,  {XLEN_BYTES} * 18(sp)",
        "lw s1,  {XLEN_BYTES} * 19(sp)",
        "lw s2,  {XLEN_BYTES} * 20(sp)",
        "lw s3,  {XLEN_BYTES} * 21(sp)",
        "lw s4,  {XLEN_BYTES} * 22(sp)",
        "lw s5,  {XLEN_BYTES} * 23(sp)",
        "lw s6,  {XLEN_BYTES} * 24(sp)",
        "lw s7,  {XLEN_BYTES} * 25(sp)",
        "lw s8,  {XLEN_BYTES} * 26(sp)",
        "lw s9,  {XLEN_BYTES} * 27(sp)",
        "lw s10, {XLEN_BYTES} * 28(sp)",
        "lw s11, {XLEN_BYTES} * 29(sp)",
        "lw sp,  {XLEN_BYTES} * 30(sp)",
        "",
        "sret",
        XLEN_BYTES = const XLEN_BYTES,
        arch_handle_trap = sym _arch_handle_trap,
    );
}

#[cfg(target_arch = "riscv64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn _arch_stvec() -> ! {
    naked_asm!(
        "csrw sscratch, sp",
        "",
        "addi sp, sp, -{XLEN_BYTES} * 31",
        "sd ra,  {XLEN_BYTES} * 0(sp)",
        "sd gp,  {XLEN_BYTES} * 1(sp)",
        "sd tp,  {XLEN_BYTES} * 2(sp)",
        "sd t0,  {XLEN_BYTES} * 3(sp)",
        "sd t1,  {XLEN_BYTES} * 4(sp)",
        "sd t2,  {XLEN_BYTES} * 5(sp)",
        "sd t3,  {XLEN_BYTES} * 6(sp)",
        "sd t4,  {XLEN_BYTES} * 7(sp)",
        "sd t5,  {XLEN_BYTES} * 8(sp)",
        "sd t6,  {XLEN_BYTES} * 9(sp)",
        "sd a0,  {XLEN_BYTES} * 10(sp)",
        "sd a1,  {XLEN_BYTES} * 11(sp)",
        "sd a2,  {XLEN_BYTES} * 12(sp)",
        "sd a3,  {XLEN_BYTES} * 13(sp)",
        "sd a4,  {XLEN_BYTES} * 14(sp)",
        "sd a5,  {XLEN_BYTES} * 15(sp)",
        "sd a6,  {XLEN_BYTES} * 16(sp)",
        "sd a7,  {XLEN_BYTES} * 17(sp)",
        "sd s0,  {XLEN_BYTES} * 18(sp)",
        "sd s1,  {XLEN_BYTES} * 19(sp)",
        "sd s2,  {XLEN_BYTES} * 20(sp)",
        "sd s3,  {XLEN_BYTES} * 21(sp)",
        "sd s4,  {XLEN_BYTES} * 22(sp)",
        "sd s5,  {XLEN_BYTES} * 23(sp)",
        "sd s6,  {XLEN_BYTES} * 24(sp)",
        "sd s7,  {XLEN_BYTES} * 25(sp)",
        "sd s8,  {XLEN_BYTES} * 26(sp)",
        "sd s9,  {XLEN_BYTES} * 27(sp)",
        "sd s10, {XLEN_BYTES} * 28(sp)",
        "sd s11, {XLEN_BYTES} * 29(sp)",
        "",
        "csrr a0, sscratch",
        "sd a0, {XLEN_BYTES} * 30(sp)",
        "",
        "mv a0, sp",
        "call {arch_handle_trap}",
        "",
        "ld ra,  {XLEN_BYTES} * 0(sp)",
        "ld gp,  {XLEN_BYTES} * 1(sp)",
        "ld tp,  {XLEN_BYTES} * 2(sp)",
        "ld t0,  {XLEN_BYTES} * 3(sp)",
        "ld t1,  {XLEN_BYTES} * 4(sp)",
        "ld t2,  {XLEN_BYTES} * 5(sp)",
        "ld t3,  {XLEN_BYTES} * 6(sp)",
        "ld t4,  {XLEN_BYTES} * 7(sp)",
        "ld t5,  {XLEN_BYTES} * 8(sp)",
        "ld t6,  {XLEN_BYTES} * 9(sp)",
        "ld a0,  {XLEN_BYTES} * 10(sp)",
        "ld a1,  {XLEN_BYTES} * 11(sp)",
        "ld a2,  {XLEN_BYTES} * 12(sp)",
        "ld a3,  {XLEN_BYTES} * 13(sp)",
        "ld a4,  {XLEN_BYTES} * 14(sp)",
        "ld a5,  {XLEN_BYTES} * 15(sp)",
        "ld a6,  {XLEN_BYTES} * 16(sp)",
        "ld a7,  {XLEN_BYTES} * 17(sp)",
        "ld s0,  {XLEN_BYTES} * 18(sp)",
        "ld s1,  {XLEN_BYTES} * 19(sp)",
        "ld s2,  {XLEN_BYTES} * 20(sp)",
        "ld s3,  {XLEN_BYTES} * 21(sp)",
        "ld s4,  {XLEN_BYTES} * 22(sp)",
        "ld s5,  {XLEN_BYTES} * 23(sp)",
        "ld s6,  {XLEN_BYTES} * 24(sp)",
        "ld s7,  {XLEN_BYTES} * 25(sp)",
        "ld s8,  {XLEN_BYTES} * 26(sp)",
        "ld s9,  {XLEN_BYTES} * 27(sp)",
        "ld s10, {XLEN_BYTES} * 28(sp)",
        "ld s11, {XLEN_BYTES} * 29(sp)",
        "ld sp,  {XLEN_BYTES} * 30(sp)",
        "",
        "sret",
        XLEN_BYTES = const XLEN_BYTES,
        arch_handle_trap = sym _arch_handle_trap,
    );
}

unsafe fn _arch_handle_trap(ctx: &ExceptionContext) {
    unsafe {
        let scause = CSR::SCAUSE.read();
        if (scause as isize) < 0 {
            match scause & 0x7fff_ffff {
                0x0000_0005 => {
                    // supervisor timer
                    timer::PlatformTimer::advance_tick();
                    return;
                }
                _ => {}
            }
        }

        let stval = CSR::STVAL.read();
        let user_pc = CSR::SEPC.read();

        println!(
            "\n\x1b[0;30;101m#### UNHANDLED EXCEPTION {:08x}, stval={:08x}, sepc={:08x}",
            scause, stval, user_pc,
        );
        println!(
            "ra {:016x} gp {:016x} tp {:016x} t0 {:016x}",
            ctx.ra, ctx.gp, ctx.tp, ctx.t0,
        );
        println!(
            "t1 {:016x} t2 {:016x} t3 {:016x} t4 {:016x}",
            ctx.t1, ctx.t2, ctx.t3, ctx.t4,
        );
        println!(
            "t5 {:016x} t6 {:016x} a0 {:016x} a1 {:016x}",
            ctx.t5, ctx.t6, ctx.a0, ctx.a1,
        );
        println!(
            "a2 {:016x} a3 {:016x} a4 {:016x} a5 {:016x}",
            ctx.a2, ctx.a3, ctx.a4, ctx.a5,
        );
        println!(
            "a6 {:016x} a7 {:016x} s0 {:016x} s1 {:016x}",
            ctx.a6, ctx.a7, ctx.s0, ctx.s1,
        );
        println!(
            "s2 {:016x} s3 {:016x} s4 {:016x} s5 {:016x}",
            ctx.s2, ctx.s3, ctx.s4, ctx.s5,
        );
        println!(
            "s6 {:016x} s7 {:016x} s8 {:016x} s9 {:016x}",
            ctx.s6, ctx.s7, ctx.s8, ctx.s9,
        );
        println!(
            "s10 {:016x} s11 {:016x} sp {:016x}",
            ctx.s10, ctx.s11, ctx.sp,
        );

        Platform::halt();
    }
}

#[repr(C)]
#[allow(dead_code)]
#[derive(Debug)]
struct ExceptionContext {
    pub ra: usize,
    pub gp: usize,
    pub tp: usize,
    pub t0: usize,
    pub t1: usize,
    pub t2: usize,
    pub t3: usize,
    pub t4: usize,
    pub t5: usize,
    pub t6: usize,
    pub a0: usize,
    pub a1: usize,
    pub a2: usize,
    pub a3: usize,
    pub a4: usize,
    pub a5: usize,
    pub a6: usize,
    pub a7: usize,
    pub s0: usize,
    pub s1: usize,
    pub s2: usize,
    pub s3: usize,
    pub s4: usize,
    pub s5: usize,
    pub s6: usize,
    pub s7: usize,
    pub s8: usize,
    pub s9: usize,
    pub s10: usize,
    pub s11: usize,
    pub sp: usize,
}

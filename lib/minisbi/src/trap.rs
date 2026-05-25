//! Trap handling
use riscv::XLEN_BYTES;

use crate::sbi_ecall::sbi_shutdown;
use crate::*;

pub(crate) unsafe fn init() {
    unsafe {
        CSR::set_mtvec(VectorMode::Direct, _mtvec as *const () as usize);

        // qemu 00f0b509
        CSR::MEDELEG.set(0b1111_0000_1011_0001_1111_1111);
    }
}

#[cfg(target_arch = "riscv32")]
#[unsafe(naked)]
unsafe extern "C" fn _mtvec() -> ! {
    naked_asm!(
        "csrw mscratch, sp",
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
        "csrr a0, mscratch",
        "sw a0, {XLEN_BYTES} * 30(sp)",
        "",
        "mv a0, sp",
        "call {handle_trap}",
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
        "mret",
        XLEN_BYTES = const XLEN_BYTES,
        handle_trap = sym _handle_trap,
    );
}

unsafe fn _handle_trap(ctx: &mut ExceptionContext) {
    unsafe {
        let mcause = CSR::MCAUSE.read();
        if (mcause as isize) < 0 {
            match mcause & 0x7fff_ffff {
                7 => {
                    // machine timer interrupt
                    timer::handle_timer_interrupt();
                    return;
                }
                _ => {}
            }
        }
        match mcause {
            9 => {
                // ecall from S mode, which is used for SBI calls
                sbi_ecall::ecall(ctx);
                return;
            }
            _ => {}
        }

        // otherwise, print the trap info and halt

        let mtval = CSR::MTVAL.read();
        let mepc = CSR::MEPC.read();
        let mstatus = CSR::MSTATUS.read();

        println!(
            "\n\x1b[0;30;101m#### EXCEPTION {:08x}, mtval={:08x}, mepc={:08x}, mstatus={:08x}",
            mcause, mtval, mepc, mstatus,
        );
        println!(
            "ra {:08x} gp {:08x} tp {:08x} t0 {:08x} t1 {:08x} t2 {:08x}",
            ctx.ra, ctx.gp, ctx.tp, ctx.t0, ctx.t1, ctx.t2,
        );
        println!(
            "t3 {:08x} t4 {:08x} t5 {:08x} t6 {:08x} a0 {:08x} a1 {:08x}",
            ctx.t3, ctx.t4, ctx.t5, ctx.t6, ctx.a0, ctx.a1,
        );
        println!(
            "a2 {:08x} a3 {:08x} a4 {:08x} a5 {:08x} a6 {:08x} a7 {:08x}",
            ctx.a2, ctx.a3, ctx.a4, ctx.a5, ctx.a6, ctx.a7,
        );
        println!(
            "s0 {:08x} s1 {:08x} s2 {:08x} s3 {:08x} s4 {:08x} s5 {:08x}",
            ctx.s0, ctx.s1, ctx.s2, ctx.s3, ctx.s4, ctx.s5,
        );
        println!(
            "s6 {:08x} s7 {:08x} s8 {:08x} s9 {:08x} s10 {:08x} s11 {:08x}",
            ctx.s6, ctx.s7, ctx.s8, ctx.s9, ctx.s10, ctx.s11,
        );
        println!("sp {:08x}", ctx.sp,);
        println!("\x1b[0m");

        sbi_shutdown();
    }
}

#[repr(C)]
#[allow(dead_code)]
#[derive(Debug)]
pub struct ExceptionContext {
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

//! Platform dependent module for riscv sbi generic (temp)

use super::*;
use crate::{
    arch::{cpu, csr::CSR},
    *,
};
use core::{arch::naked_asm, ffi::c_void};

mod sbi_console;

unsafe extern "C" {
    unsafe static _end: c_void;
}

impl PlatformTrait for Platform {
    unsafe fn init_dt_early(dt: &fdt::DeviceTree, arg: usize) {
        let hart_id = arg;
        unsafe {
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
            boot_info.platform = Platform::OpenSbi;

            let end = PhysicalAddress::new(&_end as *const _ as PhysicalAddressRepr);
            boot_info.start_conventional_memory =
                end.rounding_up(mem::MemoryManager::PAGE_SIZE).as_repr() as u32;
            boot_info.conventional_memory_size = 0x40_0000;

            println!("Model: {}", dt.root().model());
            for item in dt.root().compatible().unwrap() {
                println!("compatible: {}", item);
            }

            CSR::STVEC.write(_arch_stvec as *const () as usize);
            CSR::SIE.set(1 << 5);
            sbi::legacy::set_timer(1);
        }
    }

    unsafe fn init(_arg: usize) {
        // TODO:
        println!("-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-=-");
        // unsafe {
        //     Hal::cpu().enable_interrupt();
        // }
    }

    unsafe fn exit() {
        // TODO:
    }

    fn reset_system() -> ! {
        sbi::legacy::shutdown();
    }
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn _arch_stvec() -> ! {
    naked_asm!(
        "
    csrw sscratch, sp

    addi sp, sp, -{XLEN_BYTES} * 31
    sw ra,  {XLEN_BYTES} * 0(sp)
    sw gp,  {XLEN_BYTES} * 1(sp)
    sw tp,  {XLEN_BYTES} * 2(sp)
    sw t0,  {XLEN_BYTES} * 3(sp)
    sw t1,  {XLEN_BYTES} * 4(sp)
    sw t2,  {XLEN_BYTES} * 5(sp)
    sw t3,  {XLEN_BYTES} * 6(sp)
    sw t4,  {XLEN_BYTES} * 7(sp)
    sw t5,  {XLEN_BYTES} * 8(sp)
    sw t6,  {XLEN_BYTES} * 9(sp)
    sw a0,  {XLEN_BYTES} * 10(sp)
    sw a1,  {XLEN_BYTES} * 11(sp)
    sw a2,  {XLEN_BYTES} * 12(sp)
    sw a3,  {XLEN_BYTES} * 13(sp)
    sw a4,  {XLEN_BYTES} * 14(sp)
    sw a5,  {XLEN_BYTES} * 15(sp)
    sw a6,  {XLEN_BYTES} * 16(sp)
    sw a7,  {XLEN_BYTES} * 17(sp)
    sw s0,  {XLEN_BYTES} * 18(sp)
    sw s1,  {XLEN_BYTES} * 19(sp)
    sw s2,  {XLEN_BYTES} * 20(sp)
    sw s3,  {XLEN_BYTES} * 21(sp)
    sw s4,  {XLEN_BYTES} * 22(sp)
    sw s5,  {XLEN_BYTES} * 23(sp)
    sw s6,  {XLEN_BYTES} * 24(sp)
    sw s7,  {XLEN_BYTES} * 25(sp)
    sw s8,  {XLEN_BYTES} * 26(sp)
    sw s9,  {XLEN_BYTES} * 27(sp)
    sw s10, {XLEN_BYTES} * 28(sp)
    sw s11, {XLEN_BYTES} * 29(sp)

    csrr a0, sscratch
    sw a0, {XLEN_BYTES} * 30(sp)

    mv a0, sp
    call {arch_handle_trap}

    lw ra,  {XLEN_BYTES} * 0(sp)
    lw gp,  {XLEN_BYTES} * 1(sp)
    lw tp,  {XLEN_BYTES} * 2(sp)
    lw t0,  {XLEN_BYTES} * 3(sp)
    lw t1,  {XLEN_BYTES} * 4(sp)
    lw t2,  {XLEN_BYTES} * 5(sp)
    lw t3,  {XLEN_BYTES} * 6(sp)
    lw t4,  {XLEN_BYTES} * 7(sp)
    lw t5,  {XLEN_BYTES} * 8(sp)
    lw t6,  {XLEN_BYTES} * 9(sp)
    lw a0,  {XLEN_BYTES} * 10(sp)
    lw a1,  {XLEN_BYTES} * 11(sp)
    lw a2,  {XLEN_BYTES} * 12(sp)
    lw a3,  {XLEN_BYTES} * 13(sp)
    lw a4,  {XLEN_BYTES} * 14(sp)
    lw a5,  {XLEN_BYTES} * 15(sp)
    lw a6,  {XLEN_BYTES} * 16(sp)
    lw a7,  {XLEN_BYTES} * 17(sp)
    lw s0,  {XLEN_BYTES} * 18(sp)
    lw s1,  {XLEN_BYTES} * 19(sp)
    lw s2,  {XLEN_BYTES} * 20(sp)
    lw s3,  {XLEN_BYTES} * 21(sp)
    lw s4,  {XLEN_BYTES} * 22(sp)
    lw s5,  {XLEN_BYTES} * 23(sp)
    lw s6,  {XLEN_BYTES} * 24(sp)
    lw s7,  {XLEN_BYTES} * 25(sp)
    lw s8,  {XLEN_BYTES} * 26(sp)
    lw s9,  {XLEN_BYTES} * 27(sp)
    lw s10, {XLEN_BYTES} * 28(sp)
    lw s11, {XLEN_BYTES} * 29(sp)
    lw sp,  {XLEN_BYTES} * 30(sp)

    sret
    ",
        XLEN_BYTES = const cpu::XLEN_BYTES,
        arch_handle_trap = sym _arch_handle_trap,
    );
}

unsafe fn _arch_handle_trap(context: &ExceptionContext) {
    unsafe {
        let scause = CSR::SCAUSE.read();
        let stval = CSR::STVAL.read();
        let user_pc = CSR::SEPC.read();

        println!("\n\x1b[0;30;101m#### UNHANDLED EXCEPTION ####");
        println!(
            "scause={:08x}, stval={:08x}, sepc={:016x}",
            scause, stval, user_pc,
        );
        println!(
            "ra {:016x} gp {:016x} tp {:016x} t0 {:016x}",
            context.ra, context.gp, context.tp, context.t0,
        );
        println!(
            "t1 {:016x} t2 {:016x} t3 {:016x} t4 {:016x}",
            context.t1, context.t2, context.t3, context.t4,
        );
        println!(
            "t5 {:016x} t6 {:016x} a0 {:016x} a1 {:016x}",
            context.t5, context.t6, context.a0, context.a1,
        );
        println!(
            "a2 {:016x} a3 {:016x} a4 {:016x} a5 {:016x}",
            context.a2, context.a3, context.a4, context.a5,
        );
        println!(
            "a6 {:016x} a7 {:016x} s0 {:016x} s1 {:016x}",
            context.a6, context.a7, context.s0, context.s1,
        );
        println!(
            "s2 {:016x} s3 {:016x} s4 {:016x} s5 {:016x}",
            context.s2, context.s3, context.s4, context.s5,
        );
        println!(
            "s6 {:016x} s7 {:016x} s8 {:016x} s9 {:016x}",
            context.s6, context.s7, context.s8, context.s9,
        );
        println!(
            "s10 {:016x} s11 {:016x} sp {:016x}",
            context.s10, context.s11, context.sp,
        );
        sbi::legacy::shutdown()
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

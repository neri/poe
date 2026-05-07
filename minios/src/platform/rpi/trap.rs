use crate::{platform::rpi::timer_eoi, *};
use core::arch::{asm, naked_asm};

pub(super) unsafe fn init() {
    unsafe {
        asm!(
            "ldr {0}, =_vector_table",
            "msr vbar_el1, {0}",
            "isb",
            out(reg) _,
        );
    }
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.boot")]
#[allow(named_asm_labels)]
unsafe extern "C" fn _vector_table_nkf() {
    naked_asm!(
        ".align 11",
        "_vector_table:",
        "    brk #1",
        "",
        ".align 7",
        "    brk #1",
        "",
        ".align 7",
        "    brk #1",
        "",
        ".align 7",
        "    brk #1",
        "",
        ".align 7",
        "    // synchronous",
        "    sub sp, sp, #256",
        "    stp x0, x1, [sp, #16 * 0]",
        "    stp x2, x3, [sp, #16 * 1]",
        "    stp x4, x5, [sp, #16 * 2]",
        "    stp x6, x7, [sp, #16 * 3]",
        "    stp x8, x9, [sp, #16 * 4]",
        "    stp x10, x11, [sp, #16 * 5]",
        "    stp x12, x13, [sp, #16 * 6]",
        "    stp x14, x15, [sp, #16 * 7]",
        "    stp x16, x17, [sp, #16 * 8]",
        "    stp x18, x19, [sp, #16 * 9]",
        "    stp x20, x21, [sp, #16 * 10]",
        "    stp x22, x23, [sp, #16 * 11]",
        "    stp x24, x25, [sp, #16 * 12]",
        "    stp x26, x27, [sp, #16 * 13]",
        "    stp x28, x29, [sp, #16 * 14]",
        "    str x30, [sp, #16 * 15]",
        "",
        "    mov x0, sp",
        "    bl {_handle_exception}",
        "    b _exc_handler",
        "",
        ".align 7",
        "    // IRQ",
        "    sub sp, sp, #256",
        "    stp x0, x1, [sp, #16 * 0]",
        "    stp x2, x3, [sp, #16 * 1]",
        "    stp x4, x5, [sp, #16 * 2]",
        "    stp x6, x7, [sp, #16 * 3]",
        "    stp x8, x9, [sp, #16 * 4]",
        "    stp x10, x11, [sp, #16 * 5]",
        "    stp x12, x13, [sp, #16 * 6]",
        "    stp x14, x15, [sp, #16 * 7]",
        "    stp x16, x17, [sp, #16 * 8]",
        "    stp x18, x19, [sp, #16 * 9]",
        "    stp x20, x21, [sp, #16 * 10]",
        "    stp x22, x23, [sp, #16 * 11]",
        "    stp x24, x25, [sp, #16 * 12]",
        "    stp x26, x27, [sp, #16 * 13]",
        "    stp x28, x29, [sp, #16 * 14]",
        "    str x30, [sp, #16 * 15]",
        "",
        "    mov x0, sp",
        "    bl {_handle_irq}",
        "    b _exc_handler",
        "",
        ".align 7",
        "    // FIQ",
        "    brk #1",
        "",
        ".align 7",
        "    // SError",
        "    brk #1",
        "",
        "_exc_handler:",
        "    ldp x0, x1, [sp, #16 * 0]",
        "    ldp x2, x3, [sp, #16 * 1]",
        "    ldp x4, x5, [sp, #16 * 2]",
        "    ldp x6, x7, [sp, #16 * 3]",
        "    ldp x8, x9, [sp, #16 * 4]",
        "    ldp x10, x11, [sp, #16 * 5]",
        "    ldp x12, x13, [sp, #16 * 6]",
        "    ldp x14, x15, [sp, #16 * 7]",
        "    ldp x16, x17, [sp, #16 * 8]",
        "    ldp x18, x19, [sp, #16 * 9]",
        "    ldp x20, x21, [sp, #16 * 10]",
        "    ldp x22, x23, [sp, #16 * 11]",
        "    ldp x24, x25, [sp, #16 * 12]",
        "    ldp x26, x27, [sp, #16 * 13]",
        "    ldp x28, x29, [sp, #16 * 14]",
        "    ldr x30, [sp, #16 * 15] ",
        "    add sp, sp, #256",
        "    eret",
        _handle_irq = sym _handle_irq,
        _handle_exception = sym _handle_exception,
    );
}

fn _handle_irq(_ctx: &mut ExceptionContext) {
    unsafe {
        let cntv_ctl_el0: usize;
        asm!("mrs {}, cntv_ctl_el0", out(reg) cntv_ctl_el0);
        if (cntv_ctl_el0 & 1) != 0 {
            arch::timer::GenericTimer::advance_tick();
            timer_eoi();
        } else {
            println!("unknown interrupt!");
        }
    }
}

fn _handle_exception(ctx: &mut ExceptionContext) {
    unsafe {
        let output = System::stdout();
        output.set_attribute(0x40);

        let esr: usize;
        asm!("mrs {}, esr_el1", out(reg)esr);
        let far: usize;
        asm!("mrs {}, far_el1", out(reg)far);
        let spsr: usize;
        asm!("mrs {}, spsr_el1", out(reg)spsr);
        let elr: usize;
        asm!("mrs {}, elr_el1", out(reg)elr);

        println!(
            "\n#### Exception ESR={:016x}, FAR={:016x}, ELR={:016x}, SPSR={:016x}",
            esr, far, elr, spsr
        );
        println!(
            "x0={:016x}, x1={:016x}, x2={:016x}, x3={:016x}",
            ctx.x0, ctx.x1, ctx.x2, ctx.x3
        );
        println!(
            "x4={:016x}, x5={:016x}, x6={:016x}, x7={:016x}",
            ctx.x4, ctx.x5, ctx.x6, ctx.x7
        );
        println!(
            "x8={:016x}, x9={:016x}, x10={:016x}, x11={:016x}",
            ctx.x8, ctx.x9, ctx.x10, ctx.x11
        );
        println!(
            "x12={:016x}, x13={:016x}, x14={:016x}, x15={:016x}",
            ctx.x12, ctx.x13, ctx.x14, ctx.x15
        );
        println!(
            "x16={:016x}, x17={:016x}, x18={:016x}, x19={:016x}",
            ctx.x16, ctx.x17, ctx.x18, ctx.x19
        );
        println!(
            "x20={:016x}, x21={:016x}, x22={:016x}, x23={:016x}",
            ctx.x20, ctx.x21, ctx.x22, ctx.x23
        );
        println!(
            "x24={:016x}, x25={:016x}, x26={:016x}, x27={:016x}",
            ctx.x24, ctx.x25, ctx.x26, ctx.x27
        );
        println!(
            "x28={:016x}, x29={:016x}, x30={:016x}, zr={:016x}",
            ctx.x28, ctx.x29, ctx.x30, ctx.zr
        );

        Hal::cpu().halt();
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ExceptionContext {
    x0: u64,
    x1: u64,
    x2: u64,
    x3: u64,
    x4: u64,
    x5: u64,
    x6: u64,
    x7: u64,
    x8: u64,
    x9: u64,
    x10: u64,
    x11: u64,
    x12: u64,
    x13: u64,
    x14: u64,
    x15: u64,
    x16: u64,
    x17: u64,
    x18: u64,
    x19: u64,
    x20: u64,
    x21: u64,
    x22: u64,
    x23: u64,
    x24: u64,
    x25: u64,
    x26: u64,
    x27: u64,
    x28: u64,
    x29: u64,
    x30: u64,
    zr: u64,
}

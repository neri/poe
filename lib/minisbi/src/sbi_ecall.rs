//! Environment call (ecall) handling for MiniSBI.

use crate::{trap::ExceptionContext, *};
use sbi::{Eid, EidFid, Fid, SbiRet};

pub unsafe fn ecall(ctx: &mut ExceptionContext) {
    #[inline(always)]
    fn sbi_ret(ctx: &mut ExceptionContext, ret: SbiRet) {
        ctx.a0 = ret.error.as_raw() as usize;
        ctx.a1 = ret.value as usize;
    }

    match Eid(ctx.a7) {
        Eid::SET_TIMER => {
            let timer_value = ((ctx.a1 as u64) << 32) | (ctx.a0 as u64);
            sbi_set_timer(timer_value);
            ctx.a0 = 0; // success
        }
        Eid::CONSOLE_PUTCHAR => {
            sbi_console_putchar(ctx.a0 as u8);
            ctx.a0 = 0; // success
        }
        Eid::CONSOLE_GETCHAR => {
            ctx.a0 = sbi_console_getchar();
        }
        Eid::CLEAR_IPI => {
            unsafe {
                CSR::SIP.clear(0b10);
            }
            ctx.a0 = 0; // success
        }
        Eid::SEND_IPI => {
            ctx.a0 = (-2isize) as usize; // not supported
        }
        Eid::REMOTE_FENCE_I => {
            ctx.a0 = (-2isize) as usize; // not supported
        }
        Eid::REMOTE_SFENCE_VMA => {
            ctx.a0 = (-2isize) as usize; // not supported
        }
        Eid::REMOTE_SFENCE_VMA_ASID => {
            ctx.a0 = (-2isize) as usize; // not supported
        }
        Eid::SHUTDOWN => {
            sbi_shutdown();
        }
        eid @ Eid::BASE => {
            let eid_fid = EidFid::new(eid, Fid(ctx.a6));
            match eid_fid {
                EidFid::GET_SPEC_VERSION => {
                    sbi_ret(ctx, sbi_get_spec_version());
                }
                EidFid::GET_IMPL_ID => {
                    sbi_ret(ctx, sbi_get_impl_id());
                }
                EidFid::GET_IMPL_VERSION => {
                    sbi_ret(ctx, sbi_get_impl_version());
                }
                EidFid::GET_MVENDORID => {
                    sbi_ret(ctx, SbiRet::ok(unsafe { CSR::MVENDORID.read() }));
                }
                EidFid::GET_MARCHID => {
                    sbi_ret(ctx, SbiRet::ok(unsafe { CSR::MARCHID.read() }));
                }
                EidFid::GET_MIMPID => {
                    sbi_ret(ctx, SbiRet::ok(unsafe { CSR::MIMPID.read() }));
                }
                _ => {
                    println!(
                        "unknown ecall a0={:08x} a1={:08x} a2={:08x} a3={:08x} a4={:08x} a5={:08x} a6={:08x} a7={:08x}",
                        ctx.a0, ctx.a1, ctx.a2, ctx.a3, ctx.a4, ctx.a5, ctx.a6, ctx.a7,
                    );
                    sbi_ret(ctx, SbiRet::err(sbi::SbiError::NotSupported, 0));
                }
            }
        }
        _ => {
            println!(
                "unknown ecall a0={:08x} a1={:08x} a2={:08x} a3={:08x} a4={:08x} a5={:08x} a6={:08x} a7={:08x}",
                ctx.a0, ctx.a1, ctx.a2, ctx.a3, ctx.a4, ctx.a5, ctx.a6, ctx.a7,
            );
            sbi_ret(ctx, SbiRet::err(sbi::SbiError::NotSupported, 0));
        }
    }

    unsafe {
        let mut mepc = CSR::MEPC.read();
        mepc += 4; // skip the ecall instruction
        CSR::MEPC.write(mepc);
    }
}

pub fn sbi_set_timer(timer_value: u64) {
    timer::set_timer(timer_value);
}

pub fn sbi_console_putchar(c: u8) {
    StdOut::stdout().write_byte(c);
}

pub fn sbi_console_getchar() -> usize {
    match StdIn::stdin().read_byte() {
        Some(byte) => byte as usize,
        None => usize::MAX, // no input available
    }
}

pub fn sbi_shutdown() -> ! {
    Syscon::PowerOff.write();
    loop {
        unsafe {
            asm!("wfi", options(nomem, nostack));
        }
    }
}

pub fn sbi_get_spec_version() -> SbiRet {
    SbiRet::ok(0x0000_0001) // version 0.1
}

pub fn sbi_get_impl_id() -> SbiRet {
    SbiRet::ok(0x1234_5678) // dummy implementation ID
}

pub fn sbi_get_impl_version() -> SbiRet {
    SbiRet::ok(0x0000_0001) // version 0.1
}

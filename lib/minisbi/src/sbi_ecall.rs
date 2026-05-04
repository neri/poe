//! Environment call (ecall) handling for MiniSBI.

use crate::{trap::ExceptionContext, *};
use minilib::unknown_enum::*;
use sbi::{Eid, EidFid, Fid, ImplementationID, ResetType, SbiRet, base::SpecVersion};

/// The current SBI specification version implemented by MiniSBI.
pub const CURRENT_SPEC_VERSION: SpecVersion = SpecVersion::new(0, 2);

/// The current SBI implementation ID.
pub const CURRENT_IMPL_ID: Unknown<ImplementationID, usize> = Unknown::unknown(0x0000_0001);

/// The current SBI implementation version.
pub const CURRENT_IMPL_VERSION: usize = 0x0000_0001;

/// Handle an SBI ecall from the S-mode.
pub unsafe fn ecall(ctx: &mut ExceptionContext) {
    #[inline(always)]
    fn sbi_ret(ctx: &mut ExceptionContext, ret: SbiRet) {
        ctx.a0 = ret.error.as_raw() as usize;
        ctx.a1 = ret.value as usize;
    }

    #[inline(always)]
    fn sbi_ret_not_supported(ctx: &mut ExceptionContext) {
        sbi_ret(ctx, SbiRet::err(sbi::SbiError::NotSupported, 0));
    }

    #[inline(always)]
    fn sbi_ret_ok(ctx: &mut ExceptionContext, value: usize) {
        sbi_ret(ctx, SbiRet::ok(value));
    }

    match Eid(ctx.a7) {
        Eid::SET_TIMER => {
            if cfg!(target_arch = "riscv32") {
                let timer_value = ((ctx.a1 as u64) << 32) | (ctx.a0 as u64);
                sbi_set_timer(timer_value);
            } else {
                sbi_set_timer(ctx.a0 as u64);
            }
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
            // NOTE: Clear IPI is deprecated because S-mode can clear sip.SSIP directly.
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
                    sbi_ret_ok(ctx, CURRENT_SPEC_VERSION.0);
                }
                EidFid::GET_IMPL_ID => {
                    sbi_ret_ok(ctx, CURRENT_IMPL_ID.as_raw());
                }
                EidFid::GET_IMPL_VERSION => {
                    sbi_ret_ok(ctx, CURRENT_IMPL_VERSION);
                }
                EidFid::PROBE_EXTENSION => {
                    let probe_eid = Eid(ctx.a0);
                    match probe_eid {
                        Eid::SET_TIMER
                        | Eid::CONSOLE_PUTCHAR
                        | Eid::CONSOLE_GETCHAR
                        | Eid::SHUTDOWN
                        | Eid::BASE
                        | Eid::TIME
                        | Eid::SYSTEM_RESET => {
                            sbi_ret_ok(ctx, 1);
                        }
                        _ => {
                            sbi_ret_ok(ctx, 0);
                        }
                    }
                }
                EidFid::GET_MVENDORID => {
                    sbi_ret_ok(ctx, unsafe { CSR::MVENDORID.read() });
                }
                EidFid::GET_MARCHID => {
                    sbi_ret_ok(ctx, unsafe { CSR::MARCHID.read() });
                }
                EidFid::GET_MIMPID => {
                    sbi_ret_ok(ctx, unsafe { CSR::MIMPID.read() });
                }
                _ => {
                    println!(
                        "unknown ecall a0={:08x} a1={:08x} a2={:08x} a3={:08x} a4={:08x} a5={:08x} a6={:08x} a7={:08x}",
                        ctx.a0, ctx.a1, ctx.a2, ctx.a3, ctx.a4, ctx.a5, ctx.a6, ctx.a7,
                    );
                    sbi_ret_not_supported(ctx);
                }
            }
        }
        eid => {
            let eid_fid = EidFid::new(eid, Fid(ctx.a6));
            match eid_fid {
                EidFid::SET_TIMER => {
                    if cfg!(target_arch = "riscv32") {
                        let timer_value = ((ctx.a1 as u64) << 32) | (ctx.a0 as u64);
                        sbi_set_timer(timer_value);
                    } else {
                        sbi_set_timer(ctx.a0 as u64);
                    }
                    sbi_ret_ok(ctx, 0);
                }
                EidFid::SYSTEM_RESET => {
                    let reset_type = Unknown::<ResetType, usize>::unknown(ctx.a0);
                    let _reset_reason = ctx.a1;
                    match reset_type.known_value().ok() {
                        Some(ResetType::ColdReset) | Some(ResetType::WarmReset) => {
                            sbi_cold_reset();
                        }
                        Some(ResetType::Shutdown) => {
                            sbi_shutdown();
                        }
                        None => {
                            sbi_ret_not_supported(ctx);
                        }
                    }
                }
                _ => {
                    sbi_ret_not_supported(ctx);
                }
            }
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

pub fn sbi_cold_reset() -> ! {
    Syscon::Reboot.write();

    loop {
        unsafe {
            asm!("wfi", options(nomem, nostack));
        }
    }
}

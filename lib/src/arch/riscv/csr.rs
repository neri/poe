//! RISCV Control and Status Registers

use core::{
    arch::asm,
    sync::atomic::{Ordering, compiler_fence},
};

/// Control and Status Registers
#[derive(Debug, Clone, Copy)]
pub struct CSR;

#[allow(dead_code)]
impl CSR {
    /// SRW `sstatus` Supervisor status register.
    pub const SSTATUS: CsrReg<0x100> = CsrReg;
    /// SRW `sie` Supervisor interrupt-enable register.
    pub const SIE: CsrReg<0x104> = CsrReg;
    /// SRW `stvec` Supervisor trap handler base address.
    pub const STVEC: CsrReg<0x105> = CsrReg;
    /// SRW `scountren` Supervisor counter enable.
    pub const SCOUNTREN: CsrReg<0x106> = CsrReg;
    /// SRW `senvcfg` Supervisor environment configuration register.
    pub const SENVCFG: CsrReg<0x10A> = CsrReg;
    /// SRW `sscratch` Scratch register for supervisor trap handlers.
    pub const SSCRATCH: CsrReg<0x140> = CsrReg;
    /// SRW `sepc` Supervisor exception program counter.
    pub const SEPC: CsrReg<0x141> = CsrReg;
    /// SRW `scause` Supervisor trap cause.
    pub const SCAUSE: CsrReg<0x142> = CsrReg;
    /// SRW `stval` Supervisor bad address or instruction.
    pub const STVAL: CsrReg<0x143> = CsrReg;
    /// SRW `sip` Supervisor interrupt pending.
    pub const SIP: CsrReg<0x144> = CsrReg;
    /// SRW `satp` Supervisor address translation and protection.
    pub const SATP: CsrReg<0x180> = CsrReg;
    /// SRW `scontext` Supervisor-mode context register.
    pub const SCONTEXT: CsrReg<0x5A8> = CsrReg;

    pub fn rdtime() -> usize {
        compiler_fence(Ordering::SeqCst);
        let result: usize;
        unsafe {
            asm!("rdtime {0}", lateout(reg) result,);
        }
        result
    }
}

pub struct CsrReg<const N: usize>;

impl<const N: usize> CsrReg<N> {
    #[inline]
    pub unsafe fn read(&self) -> usize {
        compiler_fence(Ordering::SeqCst);
        let result: usize;
        unsafe {
            asm!("csrr {0}, {csr}", lateout(reg) result, csr = const N,);
        }
        result
    }

    #[inline]
    pub unsafe fn write(&self, val: usize) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            asm!("csrw {csr}, {0}", in(reg) val, csr = const N,);
        }
    }

    #[inline]
    pub unsafe fn set(&self, bits: usize) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            asm!("csrs {csr}, {0}", in(reg) bits, csr = const N,);
        }
    }

    #[inline]
    pub unsafe fn clear(&self, bits: usize) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            asm!("csrc {csr}, {0}", in(reg) bits, csr = const N,);
        }
    }
}

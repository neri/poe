//! RISCV Control and Status Registers

use core::arch::asm;
use core::sync::atomic::{Ordering, compiler_fence};

pub mod pmp;

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

    /// MRW `mstatus` Machine status register.
    pub const MSTATUS: CsrReg<0x300> = CsrReg;
    /// MRW `misa` Machine ISA register.
    pub const MISA: CsrReg<0x301> = CsrReg;
    /// MRW `medeleg` Machine exception delegation register.
    pub const MEDELEG: CsrReg<0x302> = CsrReg;
    /// MRW `mideleg` Machine interrupt delegation register.
    pub const MIDELEG: CsrReg<0x303> = CsrReg;
    /// MRW `mie` Machine interrupt-enable register.
    pub const MIE: CsrReg<0x304> = CsrReg;
    /// MRW `mtvec` Machine trap handler base address.
    pub const MTVEC: CsrReg<0x305> = CsrReg;
    /// MRW `mcountren` Machine counter enable.
    pub const MCOUNTREN: CsrReg<0x306> = CsrReg;
    /// MRW `menvcfg` Machine environment configuration register.
    pub const MENVCFG: CsrReg<0x30A> = CsrReg;
    #[cfg(target_arch = "riscv32")]
    /// MRW `mstatush` Upper half of `mstatus` (RV32 only).
    pub const MSTATUSH: CsrReg<0x310> = CsrReg;
    #[cfg(target_arch = "riscv32")]
    /// MRW `medelegh` Upper half of `medeleg` (RV32 only).
    pub const MEDELEGH: CsrReg<0x312> = CsrReg;
    /// MRW `mscratch` Scratch register for machine trap handlers.
    pub const MSCRATCH: CsrReg<0x340> = CsrReg;
    /// MRW `mepc` Machine exception program counter.
    pub const MEPC: CsrReg<0x341> = CsrReg;
    /// MRW `mcause` Machine trap cause.
    pub const MCAUSE: CsrReg<0x342> = CsrReg;
    /// MRW `mtval` Machine bad address or instruction.
    pub const MTVAL: CsrReg<0x343> = CsrReg;
    /// MRW `mip` Machine interrupt pending.
    pub const MIP: CsrReg<0x344> = CsrReg;
    /// MRW `mtinst` Machine instruction register.
    pub const MTINST: CsrReg<0x34A> = CsrReg;

    /// MRO `mvendorid` Vendor ID.
    pub const MVENDORID: CsrReg<0xF11> = CsrReg;
    /// MRO `marchid` Architecture ID.
    pub const MARCHID: CsrReg<0xF12> = CsrReg;
    /// MRO `mimpid` Implementation ID.
    pub const MIMPID: CsrReg<0xF13> = CsrReg;
    /// MRO `mhartid` Hardware thread ID.
    pub const MHARTID: CsrReg<0xF14> = CsrReg;

    /// MRW `pmpcfg0` PMP configuration register 0.
    pub const PMPCFG0: CsrReg<0x3A0> = CsrReg;
    /// MRW `pmpcfg1` PMP configuration register 1.
    pub const PMPCFG1: CsrReg<0x3A1> = CsrReg;
    /// MRW `pmpcfg2` PMP configuration register 2.
    pub const PMPCFG2: CsrReg<0x3A2> = CsrReg;
    /// MRW `pmpcfg3` PMP configuration register 3.
    pub const PMPCFG3: CsrReg<0x3A3> = CsrReg;
    /// MRW `pmpcfg4` PMP configuration register 4.
    pub const PMPCFG4: CsrReg<0x3A4> = CsrReg;
    /// MRW `pmpcfg5` PMP configuration register 5.
    pub const PMPCFG5: CsrReg<0x3A5> = CsrReg;
    /// MRW `pmpcfg6` PMP configuration register 6.
    pub const PMPCFG6: CsrReg<0x3A6> = CsrReg;
    /// MRW `pmpcfg7` PMP configuration register 7.
    pub const PMPCFG7: CsrReg<0x3A7> = CsrReg;
    /// MRW `pmpcfg8` PMP configuration register 8.
    pub const PMPCFG8: CsrReg<0x3A8> = CsrReg;
    /// MRW `pmpcfg9` PMP configuration register 9.
    pub const PMPCFG9: CsrReg<0x3A9> = CsrReg;
    /// MRW `pmpcfg10` PMP configuration register 10.
    pub const PMPCFG10: CsrReg<0x3AA> = CsrReg;
    /// MRW `pmpcfg11` PMP configuration register 11.
    pub const PMPCFG11: CsrReg<0x3AB> = CsrReg;
    /// MRW `pmpcfg12` PMP configuration register 12.
    pub const PMPCFG12: CsrReg<0x3AC> = CsrReg;
    /// MRW `pmpcfg13` PMP configuration register 13.
    pub const PMPCFG13: CsrReg<0x3AD> = CsrReg;
    /// MRW `pmpcfg14` PMP configuration register 14.
    pub const PMPCFG14: CsrReg<0x3AE> = CsrReg;
    /// MRW `pmpcfg15` PMP configuration register 15.
    pub const PMPCFG15: CsrReg<0x3AF> = CsrReg;

    /// MRW `pmpaddr0` PMP address register 0.
    pub const PMPADDR0: CsrReg<0x3B0> = CsrReg;
    /// MRW `pmpaddr1` PMP address register 1.
    pub const PMPADDR1: CsrReg<0x3B1> = CsrReg;
    /// MRW `pmpaddr2` PMP address register 2.
    pub const PMPADDR2: CsrReg<0x3B2> = CsrReg;
    /// MRW `pmpaddr3` PMP address register 3.
    pub const PMPADDR3: CsrReg<0x3B3> = CsrReg;
    /// MRW `pmpaddr4` PMP address register 4.
    pub const PMPADDR4: CsrReg<0x3B4> = CsrReg;
    /// MRW `pmpaddr5` PMP address register 5.
    pub const PMPADDR5: CsrReg<0x3B5> = CsrReg;
    /// MRW `pmpaddr6` PMP address register 6.
    pub const PMPADDR6: CsrReg<0x3B6> = CsrReg;
    /// MRW `pmpaddr7` PMP address register 7.
    pub const PMPADDR7: CsrReg<0x3B7> = CsrReg;
    /// MRW `pmpaddr8` PMP address register 8.
    pub const PMPADDR8: CsrReg<0x3B8> = CsrReg;
    /// MRW `pmpaddr9` PMP address register 9.
    pub const PMPADDR9: CsrReg<0x3B9> = CsrReg;
    /// MRW `pmpaddr10` PMP address register 10.
    pub const PMPADDR10: CsrReg<0x3BA> = CsrReg;
    /// MRW `pmpaddr11` PMP address register 11.
    pub const PMPADDR11: CsrReg<0x3BB> = CsrReg;
    /// MRW `pmpaddr12` PMP address register 12.
    pub const PMPADDR12: CsrReg<0x3BC> = CsrReg;
    /// MRW `pmpaddr13` PMP address register 13.
    pub const PMPADDR13: CsrReg<0x3BD> = CsrReg;
    /// MRW `pmpaddr14` PMP address register 14.
    pub const PMPADDR14: CsrReg<0x3BE> = CsrReg;
    /// MRW `pmpaddr15` PMP address register 15.
    pub const PMPADDR15: CsrReg<0x3BF> = CsrReg;
    /// MRW `pmpaddr16` PMP address register 16.
    pub const PMPADDR16: CsrReg<0x3C0> = CsrReg;
    /// MRW `pmpaddr17` PMP address register 17.
    pub const PMPADDR17: CsrReg<0x3C1> = CsrReg;
    /// MRW `pmpaddr18` PMP address register 18.
    pub const PMPADDR18: CsrReg<0x3C2> = CsrReg;
    /// MRW `pmpaddr19` PMP address register 19.
    pub const PMPADDR19: CsrReg<0x3C3> = CsrReg;
    /// MRW `pmpaddr20` PMP address register 20.
    pub const PMPADDR20: CsrReg<0x3C4> = CsrReg;
    /// MRW `pmpaddr21` PMP address register 21.
    pub const PMPADDR21: CsrReg<0x3C5> = CsrReg;
    /// MRW `pmpaddr22` PMP address register 22.
    pub const PMPADDR22: CsrReg<0x3C6> = CsrReg;
    /// MRW `pmpaddr23` PMP address register 23.
    pub const PMPADDR23: CsrReg<0x3C7> = CsrReg;
    /// MRW `pmpaddr24` PMP address register 24.
    pub const PMPADDR24: CsrReg<0x3C8> = CsrReg;
    /// MRW `pmpaddr25` PMP address register 25.
    pub const PMPADDR25: CsrReg<0x3C9> = CsrReg;
    /// MRW `pmpaddr26` PMP address register 26.
    pub const PMPADDR26: CsrReg<0x3CA> = CsrReg;
    /// MRW `pmpaddr27` PMP address register 27.
    pub const PMPADDR27: CsrReg<0x3CB> = CsrReg;
    /// MRW `pmpaddr28` PMP address register 28.
    pub const PMPADDR28: CsrReg<0x3CC> = CsrReg;
    /// MRW `pmpaddr29` PMP address register 29.
    pub const PMPADDR29: CsrReg<0x3CD> = CsrReg;
    /// MRW `pmpaddr30` PMP address register 30.
    pub const PMPADDR30: CsrReg<0x3CE> = CsrReg;
    /// MRW `pmpaddr31` PMP address register 31.
    pub const PMPADDR31: CsrReg<0x3CF> = CsrReg;

    pub fn rdtime() -> u64 {
        compiler_fence(Ordering::SeqCst);
        #[cfg(target_arch = "riscv64")]
        {
            let result: u64;
            unsafe {
                asm!("rdtime {0}", lateout(reg) result,);
            }
            result
        }
        #[cfg(target_arch = "riscv32")]
        {
            let mut lo: u32;
            let mut hi: u32;
            let mut check: u32;
            unsafe {
                loop {
                    asm!(
                        "rdtimeh {0}",
                        "rdtime {1}",
                        "rdtimeh {2}",
                        lateout(reg) hi,
                        lateout(reg) lo,
                        lateout(reg) check,
                    );
                    if hi == check {
                        break;
                    }
                }
            }
            ((hi as u64) << 32) | (lo as u64)
        }
    }

    /// Set the supervisor trap handler base address and mode.
    #[inline]
    pub unsafe fn set_stvec(mode: VectorMode, addr: usize) {
        compiler_fence(Ordering::SeqCst);
        let stvec_val = (addr & !0x3) | mode as usize;
        unsafe {
            Self::STVEC.write(stvec_val);
        }
    }

    /// Set the machine trap handler base address and mode.
    #[inline]
    pub unsafe fn set_mtvec(mode: VectorMode, addr: usize) {
        compiler_fence(Ordering::SeqCst);
        let mtvec_val = (addr & !0x3) | mode as usize;
        unsafe {
            Self::MTVEC.write(mtvec_val);
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VectorMode {
    Direct = 0,
    Vectored = 1,
}

//! Hardware Abstraction Layer for riscv

use crate::arch::csr::CSR;
use crate::*;
use core::arch::asm;
use core::fmt;
use core::sync::atomic::{Ordering, compiler_fence};

impl HalTrait for Hal {
    #[inline]
    fn cpu() -> impl HalCpu {
        CpuImpl
    }
}

const STATUS_SIE: usize = 0x02;

#[derive(Clone, Copy)]
struct CpuImpl;

impl HalCpu for CpuImpl {
    #[inline]
    fn no_op(&self) {
        unsafe {
            asm!("nop", options(nomem, nostack));
        }
    }

    #[inline]
    fn bad_instruction(&self) -> ! {
        unsafe {
            asm!("unimp", options(nomem, nostack, noreturn));
        }
    }

    #[inline]
    fn wait_for_interrupt(&self) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            if cfg!(feature = "sbi") {
                asm!("wfi", options(nomem, nostack));
            } else {
                // TODO: currently wfi is not working
                asm!("nop", options(nomem, nostack));
            }
        }
    }

    #[inline]
    unsafe fn enable_interrupt(&self) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            asm!("csrsi sstatus, 0x02", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn disable_interrupt(&self) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            asm!("csrci sstatus, 0x02", options(nomem, nostack));
        }
    }

    #[inline]
    fn is_interrupt_enabled(&self) -> bool {
        compiler_fence(Ordering::SeqCst);
        unsafe { CSR::SSTATUS.read() & STATUS_SIE != 0 }
    }

    #[inline]
    unsafe fn interrupt_guard(&self) -> InterruptGuard {
        unsafe {
            let sie = STATUS_SIE;
            let mut result: usize;
            compiler_fence(Ordering::SeqCst);
            asm!(
                "csrrc {result}, sstatus, {sie}",
                sie = in(reg)sie,
                result = lateout(reg)result,
            );
            compiler_fence(Ordering::SeqCst);
            InterruptGuard::new(result & sie)
        }
    }
}

impl fmt::Debug for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if cfg!(target_pointer_width = "32") {
            write!(f, "{:08x}", self.as_usize())
        } else {
            write!(f, "{:016x}", self.as_usize())
        }
    }
}

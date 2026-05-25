//! Hardware Abstraction Layer for aarch64

use core::arch::asm;
use core::fmt;
use core::sync::atomic::{Ordering, compiler_fence};

use crate::*;

impl HalTrait for Hal {
    #[inline]
    fn cpu() -> impl HalCpu {
        CpuImpl
    }
}

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
        compiler_fence(Ordering::SeqCst);
        unsafe {
            asm!("udf #0", options(nomem, nostack, noreturn));
        }
    }

    #[inline]
    fn wait_for_interrupt(&self) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            asm!("wfi", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn enable_interrupt(&self) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            asm!("msr daifclr, #2", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn disable_interrupt(&self) {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            asm!("msr daifset, #2", options(nomem, nostack));
        }
    }

    #[inline]
    fn is_interrupt_enabled(&self) -> bool {
        compiler_fence(Ordering::SeqCst);
        unsafe {
            let daif: usize;
            asm!(
                "mrs {0}, daif",
                out(reg)daif,
                options(nomem, nostack),
            );
            (daif & 0x80) == 0
        }
    }

    #[inline]
    unsafe fn interrupt_guard(&self) -> InterruptGuard {
        unsafe {
            let old: usize;
            compiler_fence(Ordering::SeqCst);
            asm!(
                "mrs {0}, daif",
                "msr daifset, #2",
                out(reg)old,
                options(nomem, nostack),
            );
            compiler_fence(Ordering::SeqCst);
            InterruptGuard::new(((old & 0x80) == 0) as usize)
        }
    }
}

impl fmt::Debug for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.as_u64())
    }
}

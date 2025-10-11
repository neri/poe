//! Hardware Abstraction Layer for aarch64

use crate::*;
use core::arch::asm;
use core::fmt;
use core::marker::PhantomData;
use core::sync::atomic::{Ordering, compiler_fence};

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
    fn wait_for_interrupt(&self) {
        unsafe {
            asm!("wfi", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn enable_interrupt(&self) {
        unsafe {
            asm!("msr daifclr, #2", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn disable_interrupt(&self) {
        unsafe {
            asm!("msr daifset, #2", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn is_interrupt_enabled(&self) -> bool {
        todo!()
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
            InterruptGuard {
                flags: old & 0x80,
                _phatom: PhantomData,
            }
        }
    }
}

#[must_use]
pub struct InterruptGuard {
    flags: usize,
    _phatom: PhantomData<Rc<()>>,
}

impl Drop for InterruptGuard {
    #[inline]
    fn drop(&mut self) {
        compiler_fence(Ordering::SeqCst);
        if self.flags != 0 {
            unsafe {
                Hal::cpu().enable_interrupt();
            }
        }
    }
}

impl fmt::Debug for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.as_u64())
    }
}

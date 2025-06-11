//! Hardware Abstraction Layer for riscv

use crate::{arch::csr::CSR, *};
use core::{arch::asm, fmt, marker::PhantomData};

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
    fn wait_for_interrupt(&self) {
        unsafe {
            asm!("wfi", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn enable_interrupt(&self) {
        unsafe {
            asm!("csrsi sstatus, 0x02", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn disable_interrupt(&self) {
        unsafe {
            asm!("csrci sstatus, 0x02", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn is_interrupt_enabled(&self) -> bool {
        unsafe { CSR::SSTATUS.read() & STATUS_SIE != 0 }
    }

    #[inline]
    unsafe fn interrupt_guard(&self) -> InterruptGuard {
        let sie = STATUS_SIE;
        let mut result: usize;
        unsafe {
            asm!("csrrc {result}, sstatus, {sie}",
            sie = in(reg)sie,
            result = lateout(reg)result,
            );
        }
        InterruptGuard {
            flags: result & sie,
            _phatom: PhantomData,
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

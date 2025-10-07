//! Hardware Abstraction Layer for x86

use crate::*;
use core::{arch::asm, fmt, marker::PhantomData};
use x86::gpr::Flags;

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
            asm!("hlt", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn enable_interrupt(&self) {
        unsafe {
            asm!("sti", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn disable_interrupt(&self) {
        unsafe {
            asm!("cli", options(nomem, nostack));
        }
    }

    #[inline]
    unsafe fn is_interrupt_enabled(&self) -> bool {
        unsafe { Flags::read().contains(Flags::IF) }
    }

    #[cfg(target_arch = "x86")]
    #[inline]
    unsafe fn interrupt_guard(&self) -> InterruptGuard {
        let mut flags: usize;
        unsafe {
            asm!(
                "pushfd",
                "cli",
                "pop {0}",
                lateout(reg) flags,
            );
        }
        InterruptGuard {
            flags,
            _phatom: PhantomData,
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn interrupt_guard(&self) -> InterruptGuard {
        let mut flags: usize;
        unsafe {
            asm!(
                "pushfq",
                "cli",
                "pop {0}",
                lateout(reg) flags,
            );
        }
        InterruptGuard {
            flags,
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
        if Flags::from_bits_retain(self.flags).contains(Flags::IF) {
            unsafe {
                Hal::cpu().enable_interrupt();
            }
        }
    }
}

#[cfg(target_arch = "x86")]
impl fmt::Debug for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08x}", self.as_u32())
    }
}

#[cfg(target_arch = "x86_64")]
impl fmt::Debug for PhysicalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:012x}", self.as_u64())
    }
}

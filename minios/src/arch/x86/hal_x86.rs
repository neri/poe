//! Hardware Abstraction Layer for x86

use core::arch::asm;
use core::fmt;
use core::sync::atomic::{Ordering, compiler_fence};

use x86::gpr::Flags;

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
        unsafe {
            asm!("ud2", options(nomem, nostack, noreturn));
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
    fn is_interrupt_enabled(&self) -> bool {
        Flags::read().contains(Flags::IF)
    }

    #[cfg(target_arch = "x86")]
    #[inline]
    unsafe fn interrupt_guard(&self) -> InterruptGuard {
        unsafe {
            let mut flags: usize;
            compiler_fence(Ordering::SeqCst);
            asm!(
                "pushfd",
                "cli",
                "pop {0}",
                lateout(reg) flags,
            );
            compiler_fence(Ordering::SeqCst);
            InterruptGuard::new(flags & Flags::IF.bits())
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn interrupt_guard(&self) -> InterruptGuard {
        unsafe {
            let mut flags: usize;
            compiler_fence(Ordering::SeqCst);
            asm!(
                "pushfq",
                "cli",
                "pop {0}",
                lateout(reg) flags,
            );
            compiler_fence(Ordering::SeqCst);
            InterruptGuard::new(flags & Flags::IF.bits())
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

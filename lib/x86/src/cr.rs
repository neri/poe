//! Control Registers

use crate::view::ControlRegisterView;
use core::arch::asm;
use core::sync::atomic::{Ordering, compiler_fence};

/// Control Register 0
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CR0(usize);

#[allow(non_snake_case)]
impl CR0 {
    /// Protected Mode Enable
    pub fn PE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 0)
    }

    /// Monitor co-processor
    pub fn MP<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 1)
    }

    /// x87 FPU Emulation
    pub fn EM<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 2)
    }

    /// Task switched
    pub fn TS<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 3)
    }

    /// Extension type
    pub fn ET<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 4)
    }

    /// Numeric error
    pub fn NE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 5)
    }

    /// Write protect
    pub fn WP<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 16)
    }

    /// Alignment mask
    pub fn AM<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 18)
    }

    /// Not-write through
    pub fn NW<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 29)
    }

    /// Cache disable
    pub fn CD<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 30)
    }

    /// Paging Enable
    pub fn PG<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 31)
    }

    #[inline]
    pub fn fetch() -> Self {
        unsafe {
            let mut eax: usize;
            asm!("mov {0}, cr0", lateout (reg) eax);
            Self(eax)
        }
    }

    /// # SAFETY
    ///
    /// The caller must ensure that the value being written to CR0 is valid and does not cause undefined behavior.
    #[inline]
    pub unsafe fn update(&self) {
        unsafe {
            compiler_fence(Ordering::SeqCst);
            let eax = self.0;
            asm!("mov cr0, {0}", in (reg) eax);
            compiler_fence(Ordering::SeqCst);
        }
    }

    #[inline]
    pub unsafe fn fetch_update<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        unsafe {
            let mut cr0 = Self::fetch();
            let result = f(&mut cr0);
            cr0.update();
            result
        }
    }

    /// Perform a `clts` instruction, which clears the TS flag in CR0.
    #[inline]
    pub fn clts() {
        unsafe {
            compiler_fence(Ordering::SeqCst);
            asm!("clts", options(nostack, nomem));
            compiler_fence(Ordering::SeqCst);
        }
    }
}

pub struct CR2;

impl CR2 {
    #[inline]
    pub fn read() -> usize {
        unsafe {
            compiler_fence(Ordering::SeqCst);
            let mut result: usize;
            asm!("mov {}, cr2", lateout (reg) result);
            compiler_fence(Ordering::SeqCst);
            result
        }
    }
}

pub struct CR3;

impl CR3 {
    #[inline]
    pub fn read() -> usize {
        unsafe {
            compiler_fence(Ordering::SeqCst);
            let result: usize;
            asm!("mov {}, cr3", lateout (reg) result);
            compiler_fence(Ordering::SeqCst);
            result
        }
    }

    /// # SAFETY
    ///
    /// The caller must ensure that the value being written to CR3 is valid and does not cause undefined behavior.
    #[inline]
    pub unsafe fn write(value: usize) {
        unsafe {
            compiler_fence(Ordering::SeqCst);
            asm!("mov cr3, {}", in (reg) value);
            compiler_fence(Ordering::SeqCst);
        }
    }
}

/// Control Register 4
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CR4(usize);

#[allow(non_snake_case)]
impl CR4 {
    /// Virtual 8086 Mode Extensions
    pub fn VME<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 0)
    }

    /// Protected-mode Virtual Interrupts
    pub fn PVI<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 1)
    }

    /// Time Stamp Disable
    pub fn TSD<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 2)
    }

    /// Debugging Extensions
    pub fn DE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 3)
    }

    /// Page Size Extension
    pub fn PSE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 4)
    }

    /// Physical Address Extension
    pub fn PAE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 5)
    }

    /// Machine Check Exception
    pub fn MCE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 6)
    }

    /// Page Global Enabled
    pub fn PGE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 7)
    }

    /// Performance-Monitoring Counter enable
    pub fn PCE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 8)
    }

    /// Operating system support for FXSAVE and FXRSTOR instructions
    pub fn OSFXSR<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 9)
    }

    /// Operating System Support for Unmasked SIMD Floating-Point Exceptions
    pub fn OSXMMEXCPT<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 10)
    }

    /// User-Mode Instruction Prevention (if set, #GP on SGDT, SIDT, SLDT, SMSW, and STR instructions when CPL > 0)
    pub fn UMIP<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 11)
    }

    /// Virtual Machine Extensions Enable
    pub fn VMXE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 13)
    }

    /// Safer Mode Extensions Enable
    pub fn SMXE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 14)
    }

    /// Enables the instructions RDFSBASE, RDGSBASE, WRFSBASE, and WRGSBASE
    pub fn FSGSBASE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 16)
    }

    /// PCID Enable
    pub fn PCIDE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 17)
    }

    /// XSAVE and Processor Extended States Enable
    pub fn OSXSAVE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 18)
    }

    /// Supervisor Mode Execution Protection Enable
    pub fn SMEP<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 20)
    }

    /// Supervisor Mode Access Prevention Enable
    pub fn SMAP<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 21)
    }

    /// Protection Key Enable
    pub fn PKE<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 22)
    }

    /// Control-flow Enforcement Technology
    pub fn CET<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 23)
    }

    /// Enable Protection Keys for Supervisor-Mode Pages
    pub fn PKS<'a>(&'a mut self) -> ControlRegisterView<'a, usize> {
        ControlRegisterView::new(&mut self.0, 1 << 24)
    }

    #[inline]
    pub fn fetch() -> Self {
        unsafe {
            compiler_fence(Ordering::SeqCst);
            let mut eax: usize;
            asm!("mov {0}, cr4", lateout (reg) eax);
            compiler_fence(Ordering::SeqCst);
            Self(eax)
        }
    }

    #[inline]
    pub unsafe fn update(&self) {
        unsafe {
            compiler_fence(Ordering::SeqCst);
            let eax = self.0;
            asm!("mov cr4, {0}", in (reg) eax);
            compiler_fence(Ordering::SeqCst);
        }
    }

    #[inline]
    pub unsafe fn fetch_update<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        unsafe {
            let mut cr4 = Self::fetch();
            let result = f(&mut cr4);
            cr4.update();
            result
        }
    }
}

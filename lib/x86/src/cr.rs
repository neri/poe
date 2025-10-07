//! Control Registers

use core::{
    arch::asm,
    ops::{BitOr, BitOrAssign},
    sync::atomic::{Ordering, compiler_fence},
};

/// Control Register 0
#[allow(dead_code)]
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct CR0(usize);

impl CR0 {
    /// Protected Mode Enable
    pub const PE: Self = Self(1 << 0);
    /// Monitor co-processor
    pub const MP: Self = Self(1 << 1);
    /// x87 FPU Emulation
    pub const EM: Self = Self(1 << 2);
    /// Task switched
    pub const TS: Self = Self(1 << 3);
    /// Extension type
    pub const ET: Self = Self(1 << 4);
    /// Numeric error
    pub const NE: Self = Self(1 << 5);
    /// Write protect
    pub const WP: Self = Self(1 << 16);
    /// Alignment mask
    pub const AM: Self = Self(1 << 18);
    /// Not-write through
    pub const NW: Self = Self(1 << 29);
    /// Cache disable
    pub const CD: Self = Self(1 << 30);
    /// Paging Enable
    pub const PG: Self = Self(1 << 31);

    #[inline]
    pub unsafe fn fetch() -> Self {
        unsafe {
            compiler_fence(Ordering::SeqCst);
            let mut eax: usize;
            asm!("mov {0}, cr0", lateout (reg) eax);
            compiler_fence(Ordering::SeqCst);
            Self(eax)
        }
    }

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
    pub unsafe fn fetch_update<F, R>(kernel: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        unsafe {
            let mut cr0 = Self::fetch();
            let result = kernel(&mut cr0);
            cr0.update();
            result
        }
    }

    #[inline]
    pub fn enable(&mut self, other: Self) -> Self {
        self.0 |= other.0;
        *self
    }

    #[inline]
    pub fn disable(&mut self, other: Self) -> Self {
        self.0 &= !other.0;
        *self
    }

    #[inline]
    pub fn set(&mut self, other: Self, value: bool) -> Self {
        if value {
            self.enable(other)
        } else {
            self.disable(other)
        }
    }

    #[inline]
    pub fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl BitOr<Self> for CR0 {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign<Self> for CR0 {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

pub struct CR2;

impl CR2 {
    #[inline]
    pub unsafe fn read() -> usize {
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
    pub unsafe fn read() -> usize {
        unsafe {
            compiler_fence(Ordering::SeqCst);
            let result: usize;
            asm!("mov {}, cr3", lateout (reg) result);
            compiler_fence(Ordering::SeqCst);
            result
        }
    }

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
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct CR4(usize);

impl CR4 {
    /// Virtual 8086 Mode Extensions
    pub const VME: Self = Self(1 << 0);
    /// Protected-mode Virtual Interrupts
    pub const PVI: Self = Self(1 << 1);
    /// Time Stamp Disable
    pub const TSD: Self = Self(1 << 2);
    /// Debugging Extensions
    pub const DE: Self = Self(1 << 3);
    /// Page Size Extension
    pub const PSE: Self = Self(1 << 4);
    /// Physical Address Extension
    pub const PAE: Self = Self(1 << 5);
    /// Machine Check Exception
    pub const MCE: Self = Self(1 << 6);
    /// Page Global Enabled
    pub const PGE: Self = Self(1 << 7);
    /// Performance-Monitoring Counter enable
    pub const PCE: Self = Self(1 << 8);
    /// Operating system support for FXSAVE and FXRSTOR instructions
    pub const OSFXSR: Self = Self(1 << 9);
    /// Operating System Support for Unmasked SIMD Floating-Point Exceptions
    pub const OSXMMEXCPT: Self = Self(1 << 10);
    /// User-Mode Instruction Prevention (if set, #GP on SGDT, SIDT, SLDT, SMSW, and STR instructions when CPL > 0)
    pub const UMIP: Self = Self(1 << 11);
    /// Virtual Machine Extensions Enable
    pub const VMXE: Self = Self(1 << 13);
    /// Safer Mode Extensions Enable
    pub const SMXE: Self = Self(1 << 14);
    /// Enables the instructions RDFSBASE, RDGSBASE, WRFSBASE, and WRGSBASE
    pub const FSGSBASE: Self = Self(1 << 16);
    /// PCID Enable
    pub const PCIDE: Self = Self(1 << 17);
    /// XSAVE and Processor Extended States Enable
    pub const OSXSAVE: Self = Self(1 << 18);
    /// Supervisor Mode Execution Protection Enable
    pub const SMEP: Self = Self(1 << 20);
    /// Supervisor Mode Access Prevention Enable
    pub const SMAP: Self = Self(1 << 21);
    /// Protection Key Enable
    pub const PKE: Self = Self(1 << 22);
    /// Control-flow Enforcement Technology
    pub const CET: Self = Self(1 << 23);
    /// Enable Protection Keys for Supervisor-Mode Pages
    pub const PKS: Self = Self(1 << 24);

    #[inline]
    pub unsafe fn fetch() -> Self {
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
    pub unsafe fn fetch_update<F, R>(kernel: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        unsafe {
            let mut cr4 = Self::fetch();
            let result = kernel(&mut cr4);
            cr4.update();
            result
        }
    }

    #[inline]
    pub fn enable(&mut self, other: Self) -> Self {
        self.0 |= other.0;
        *self
    }

    #[inline]
    pub fn disable(&mut self, other: Self) -> Self {
        self.0 &= !other.0;
        *self
    }

    #[inline]
    pub fn set(&mut self, other: Self, value: bool) -> Self {
        if value {
            self.enable(other)
        } else {
            self.disable(other)
        }
    }

    #[inline]
    pub fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl BitOr<Self> for CR4 {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign<Self> for CR4 {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

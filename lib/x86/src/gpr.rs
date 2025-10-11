//! General Purpose Registers and Flags

use crate::{
    prot::{IOPL, Offset32},
    real::Offset16,
};
use core::{
    arch::asm,
    fmt::{self, LowerHex},
    ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign},
};

#[cfg(target_arch = "x86")]
pub type Eflags = Flags;
#[cfg(target_arch = "x86_64")]
pub type Rflags = Flags;

/// 32-bit general purpose register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Gpr32(pub u32);

impl Gpr32 {
    /// Get the 32-bit value of the register.
    #[inline]
    pub const fn d(&self) -> u32 {
        self.0
    }

    /// Set the 32-bit value of the register.
    #[inline]
    pub fn set_d(&mut self, d: u32) {
        self.0 = d;
    }

    /// Get the 16-bit value of the register.
    #[inline]
    pub const fn w(&self) -> u16 {
        self.0 as u16
    }

    /// Set the 16-bit value of the register.
    #[inline]
    pub fn set_w(&mut self, w: u16) {
        self.0 = (self.0 & 0xffff_0000) | (w as u32);
    }

    /// Get the 8-bit value of the register.
    #[inline]
    pub const fn b(&self) -> u8 {
        self.0 as u8
    }

    /// Set the 8-bit value of the register.
    #[inline]
    pub fn set_b(&mut self, b: u8) {
        self.0 = (self.0 & 0xffffff00) | (b as u32);
    }

    /// Get the high 8 bits of the register.
    #[inline]
    pub const fn h(&self) -> u8 {
        (self.0 >> 8) as u8
    }

    /// Set the high 8 bits of the register.
    #[inline]
    pub fn set_h(&mut self, h: u8) {
        self.0 = (self.0 & 0xffff00ff) | ((h as u32) << 8);
    }
}

/// 32-bit pointer register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pointer32(pub u32);

impl Pointer32 {
    #[inline]
    pub const fn from_u32(v: u32) -> Self {
        Self(v)
    }

    #[inline]
    pub const fn from_u16(w: u16) -> Self {
        Self(w as u32)
    }

    #[inline]
    pub const fn as_u32(&self) -> u32 {
        self.0
    }

    #[inline]
    pub const fn as_u16(&self) -> u16 {
        self.0 as u16
    }

    #[inline]
    pub const fn offset32(&self) -> Offset32 {
        Offset32::new(self.0)
    }

    #[inline]
    pub const fn offset16(&self) -> Offset16 {
        Offset16::new(self.0 as u16)
    }
}

impl From<u16> for Pointer32 {
    #[inline]
    fn from(value: u16) -> Self {
        Self::from_u16(value)
    }
}

impl From<u32> for Pointer32 {
    #[inline]
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<Offset16> for Pointer32 {
    #[inline]
    fn from(value: Offset16) -> Self {
        Self::from_u16(value.as_u16())
    }
}

impl From<Offset32> for Pointer32 {
    #[inline]
    fn from(value: Offset32) -> Self {
        Self::from_u32(value.as_u32())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Flags(usize);

impl Flags {
    /// Carry flag
    pub const CF: Self = Self(0x0000_0001);
    // Reserved Always 1
    pub const _VF: Self = Self(0x0000_0002);
    /// Parity flag
    pub const PF: Self = Self(0x0000_0004);
    /// Adjust flag
    pub const AF: Self = Self(0x0000_0010);
    /// Zero flag
    pub const ZF: Self = Self(0x0000_0040);
    /// Sign flag
    pub const SF: Self = Self(0x0000_0080);
    /// Trap flag
    pub const TF: Self = Self(0x0000_0100);
    /// Interrupt enable flag
    pub const IF: Self = Self(0x0000_0200);
    /// Direction flag
    pub const DF: Self = Self(0x0000_0400);
    /// Overflow flag
    pub const OF: Self = Self(0x0000_0800);
    /// I/O privilege level
    pub const IOPL3: Self = Self(0x0000_3000);
    /// Nested task flag
    pub const NT: Self = Self(0x0000_4000);
    /// Mode flag (NEC V30)
    pub const MD: Self = Self(0x0000_8000);
    /// Resume flag
    pub const RF: Self = Self(0x0001_0000);
    /// Virtual 8086 mode flag
    #[cfg(target_arch = "x86")]
    pub const VM: Self = Self(0x0002_0000);
    /// Alignment check
    pub const AC: Self = Self(0x0004_0000);
    /// Virtual interrupt flag
    pub const VIF: Self = Self(0x0008_0000);
    /// Virtual interrupt pending
    pub const VIP: Self = Self(0x0010_0000);
    /// Able to use CPUID instruction
    pub const ID: Self = Self(0x0020_0000);

    pub const ALWAYS_1_BITMAP: Self = Self::_VF;

    pub const ALWAYS_0_BITMAP: Self = Self(0x0000_8028);

    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[inline]
    pub const fn from_bits_retain(bits: usize) -> Self {
        Self(bits)
    }

    #[inline]
    pub fn from_bits(bits: usize) -> Self {
        Self::from_bits_retain(bits).canonicalized()
    }

    #[inline]
    pub const fn bits(&self) -> usize {
        self.0
    }

    #[inline]
    pub const fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[inline]
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    #[inline]
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    #[inline]
    pub fn set(&mut self, bit: Self, value: bool) {
        if value {
            self.insert(bit);
        } else {
            self.remove(bit);
        }
    }

    #[cfg(target_arch = "x86")]
    #[inline]
    pub unsafe fn read() -> Self {
        let flags: usize;
        unsafe {
            asm!(
                "pushfd",
                "pop {}",
                out(reg)flags,
            );
        }
        Self::from_bits_retain(flags)
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    pub unsafe fn read() -> Self {
        let flags: usize;
        unsafe {
            asm!(
                "pushfq",
                "pop {}",
                out(reg)flags,
            );
        }
        Self::from_bits_retain(flags)
    }

    #[inline]
    pub fn iopl(&self) -> IOPL {
        IOPL::from_flags(self.bits())
    }

    #[inline]
    pub fn set_iopl(&mut self, iopl: IOPL) {
        *self = Self::from_bits_retain((self.bits() & !Self::IOPL3.bits()) | (iopl.into_flags()))
    }

    #[inline]
    pub fn clear_iopl(&mut self) {
        self.remove(Self::IOPL3);
    }

    #[inline]
    pub const fn is_canonical(&self) -> bool {
        self.bits() & Self::ALWAYS_1_BITMAP.bits() == Self::ALWAYS_1_BITMAP.bits()
            && self.bits() & Self::ALWAYS_0_BITMAP.bits() == 0
    }

    #[inline]
    pub fn canonicalized(&self) -> Self {
        Self::from_bits_retain(
            (self.bits() & !Self::ALWAYS_0_BITMAP.bits()) | Self::ALWAYS_1_BITMAP.bits(),
        )
    }

    #[inline]
    pub fn canonicalize(&mut self) {
        *self = self.canonicalized();
    }
}

impl BitOr<Self> for Flags {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign<Self> for Flags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.insert(rhs);
    }
}

impl BitAnd<Self> for Flags {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for Flags {
    #[inline]
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl BitXor<Self> for Flags {
    type Output = Self;

    #[inline]
    fn bitxor(self, rhs: Self) -> Self::Output {
        Self(self.0 ^ rhs.0)
    }
}

impl BitXorAssign<Self> for Flags {
    #[inline]
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0;
    }
}

impl fmt::Debug for Flags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Flags({})", self)
    }
}

impl fmt::Display for Flags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08x} [", self.0)?;

        if self.contains(Self::OF) {
            write!(f, "O")?;
        } else {
            write!(f, "-")?;
        }

        if self.contains(Self::DF) {
            write!(f, "D")?;
        } else {
            write!(f, "-")?;
        }

        if self.contains(Self::IF) {
            write!(f, "I")?;
        } else {
            write!(f, "-")?;
        }

        if self.contains(Self::SF) {
            write!(f, "S")?;
        } else {
            write!(f, "-")?;
        }

        if self.contains(Self::ZF) {
            write!(f, "Z")?;
        } else {
            write!(f, "-")?;
        }

        if self.contains(Self::AF) {
            write!(f, "A")?;
        } else {
            write!(f, "-")?;
        }

        if self.contains(Self::PF) {
            write!(f, "P")?;
        } else {
            write!(f, "-")?;
        }

        if self.contains(Self::CF) {
            write!(f, "C")?;
        } else {
            write!(f, "-")?;
        }

        write!(f, "]")
    }
}

impl LowerHex for Flags {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

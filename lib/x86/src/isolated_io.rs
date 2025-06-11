//! Isolated I/O operations.

use core::arch::asm;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct IoPort(pub u16);

impl IoPort {
    #[inline]
    pub const fn new(port: u16) -> Self {
        Self(port)
    }

    #[inline(always)]
    pub unsafe fn out8(&self, value: u8) {
        unsafe {
            asm!(
                "out dx, al",
                in("dx") self.0,
                in("al") value,
            );
        }
    }

    #[inline(always)]
    pub unsafe fn out16(&self, value: u16) {
        unsafe {
            asm!(
                "out dx, ax",
                in("dx") self.0,
                in("ax") value,
            );
        }
    }

    #[inline(always)]
    pub unsafe fn out32(&self, value: u32) {
        unsafe {
            asm!(
                "out dx, eax",
                in("dx") self.0,
                in("eax") value,
            );
        }
    }

    #[inline(always)]
    pub unsafe fn in8(&self) -> u8 {
        let value: u8;
        unsafe {
            asm!(
                "in al, dx",
                out("al") value,
                in("dx") self.0,
            );
        }
        value
    }

    #[inline(always)]
    pub unsafe fn in16(&self) -> u16 {
        let value: u16;
        unsafe {
            asm!(
                "in ax, dx",
                out("ax") value,
                in("dx") self.0,
            );
        }
        value
    }

    #[inline(always)]
    pub unsafe fn in32(&self) -> u32 {
        let value: u32;
        unsafe {
            asm!(
                "in eax, dx",
                out("eax") value,
                in("dx") self.0,
            );
        }
        value
    }
}

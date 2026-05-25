//! Physical Memory Protection (PMP) implementation for RISC-V.

use core::arch::naked_asm;

use super::*;

/// Physical Memory Protection (PMP)
pub struct Pmp;

/// PMP implementations may implement zero, 16, or 64 PMP entries.
/// This enum is used to identify the PMP implementation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PmpImpl {
    /// No PMP support
    Zero = 0,
    /// PMP has 16 entries
    Pmp16 = 1,
    /// PMP has 64 entries
    Pmp64 = 2,
}

impl PmpImpl {
    /// Identify the PMP implementation
    ///
    /// # SAFETY
    ///
    /// * This function overwrites `mtvec` CSR.
    #[unsafe(naked)]
    pub unsafe extern "C" fn identify() -> PmpImpl {
        naked_asm!(
            "  la t0, 100f",
            "  csrw mtvec, t0",
            "  csrr t1, pmpaddr15",
            "  la t0, 200f",
            "  csrw mtvec, t0",
            "  csrr t1, pmpaddr63",
            "  li a0, 2",
            "  ret",
            "",
            ".align 4",
            "100:",
            "  li a0, 0",
            "  ret",
            "",
            ".align 4",
            "200:",
            "  li a0, 1",
            "  ret",
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PmpIndex {
    Pmp0 = 0,
    Pmp1 = 1,
    Pmp2 = 2,
    Pmp3 = 3,
    Pmp4 = 4,
    Pmp5 = 5,
    Pmp6 = 6,
    Pmp7 = 7,
    Pmp8 = 8,
    Pmp9 = 9,
    Pmp10 = 10,
    Pmp11 = 11,
    Pmp12 = 12,
    Pmp13 = 13,
    Pmp14 = 14,
    Pmp15 = 15,
    // TODO: add more PMP entries if supported by the implementation
}

#[derive(Debug, Clone, Copy)]
pub struct PmpConfig(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmpAddressMode {
    Off = 0,
    Tor = 1,
    Na4 = 2,
    Napot = 3,
}

impl PmpIndex {
    pub const ALL: [Self; 16] = [
        Self::Pmp0,
        Self::Pmp1,
        Self::Pmp2,
        Self::Pmp3,
        Self::Pmp4,
        Self::Pmp5,
        Self::Pmp6,
        Self::Pmp7,
        Self::Pmp8,
        Self::Pmp9,
        Self::Pmp10,
        Self::Pmp11,
        Self::Pmp12,
        Self::Pmp13,
        Self::Pmp14,
        Self::Pmp15,
    ];

    #[inline]
    pub fn iter() -> impl Iterator<Item = PmpIndex> {
        Self::ALL.iter().copied()
    }

    /// Reads the PMP configuration and address for this PMP index. Returns `None` if the PMP entry is disabled.
    pub unsafe fn read(&self) -> Option<(PmpConfig, usize)> {
        unsafe {
            let cfg = if cfg!(target_arch = "riscv32") {
                let raw_val = Self::read_cfg(*self as usize / 4) as u32;
                let shift = (*self as usize % 4) * 8;
                let val = (raw_val >> shift) & 0xff;
                PmpConfig(val as u8)
            } else {
                let raw_val = Self::read_cfg((*self as usize / 8) * 2) as u64;
                let shift = (*self as usize % 8) * 8;
                let val = (raw_val >> shift) & 0xff;
                PmpConfig(val as u8)
            };
            match cfg.addr_mode() {
                PmpAddressMode::Off => None,
                _ => {
                    let addr = self.read_addr();
                    Some((cfg, addr))
                }
            }
        }
    }

    /// Writes the PMP configuration and address for this PMP index.
    pub unsafe fn write(&self, cfg: PmpConfig, addr: usize) {
        unsafe {
            if cfg!(target_arch = "riscv32") {
                let raw_val = Self::read_cfg(*self as usize / 4) as u32;
                let shift = (*self as usize % 4) * 8;
                let mask = !(0xff << shift);
                let new_val = (raw_val & mask) | ((cfg.0 as u32) << shift);
                Self::write_cfg(*self as usize / 4, new_val as usize);
            } else {
                let raw_val = Self::read_cfg((*self as usize / 8) * 2) as u64;
                let shift = (*self as usize % 8) * 8;
                let mask = !(0xff << shift);
                let new_val = (raw_val & mask) | ((cfg.0 as u64) << shift);
                Self::write_cfg((*self as usize / 8) * 2, new_val as usize);
            }
            self.write_addr(addr);
        }
    }

    #[inline]
    pub unsafe fn write_off(&self) {
        unsafe {
            self.write(PmpConfig(0), 0);
        }
    }

    unsafe fn read_cfg(index: usize) -> usize {
        unsafe {
            match index {
                0 => CSR::PMPCFG0.read(),
                1 => CSR::PMPCFG1.read(),
                2 => CSR::PMPCFG2.read(),
                3 => CSR::PMPCFG3.read(),
                4 => CSR::PMPCFG4.read(),
                5 => CSR::PMPCFG5.read(),
                6 => CSR::PMPCFG6.read(),
                7 => CSR::PMPCFG7.read(),
                8 => CSR::PMPCFG8.read(),
                9 => CSR::PMPCFG9.read(),
                10 => CSR::PMPCFG10.read(),
                11 => CSR::PMPCFG11.read(),
                12 => CSR::PMPCFG12.read(),
                13 => CSR::PMPCFG13.read(),
                14 => CSR::PMPCFG14.read(),
                15 => CSR::PMPCFG15.read(),
                _ => unreachable!(),
            }
        }
    }

    unsafe fn write_cfg(index: usize, value: usize) {
        unsafe {
            match index {
                0 => CSR::PMPCFG0.write(value),
                1 => CSR::PMPCFG1.write(value),
                2 => CSR::PMPCFG2.write(value),
                3 => CSR::PMPCFG3.write(value),
                4 => CSR::PMPCFG4.write(value),
                5 => CSR::PMPCFG5.write(value),
                6 => CSR::PMPCFG6.write(value),
                7 => CSR::PMPCFG7.write(value),
                8 => CSR::PMPCFG8.write(value),
                9 => CSR::PMPCFG9.write(value),
                10 => CSR::PMPCFG10.write(value),
                11 => CSR::PMPCFG11.write(value),
                12 => CSR::PMPCFG12.write(value),
                13 => CSR::PMPCFG13.write(value),
                14 => CSR::PMPCFG14.write(value),
                15 => CSR::PMPCFG15.write(value),
                _ => unreachable!(),
            }
        }
    }

    #[inline]
    pub unsafe fn read_addr(&self) -> usize {
        unsafe {
            match self {
                PmpIndex::Pmp0 => CSR::PMPADDR0.read(),
                PmpIndex::Pmp1 => CSR::PMPADDR1.read(),
                PmpIndex::Pmp2 => CSR::PMPADDR2.read(),
                PmpIndex::Pmp3 => CSR::PMPADDR3.read(),
                PmpIndex::Pmp4 => CSR::PMPADDR4.read(),
                PmpIndex::Pmp5 => CSR::PMPADDR5.read(),
                PmpIndex::Pmp6 => CSR::PMPADDR6.read(),
                PmpIndex::Pmp7 => CSR::PMPADDR7.read(),
                PmpIndex::Pmp8 => CSR::PMPADDR8.read(),
                PmpIndex::Pmp9 => CSR::PMPADDR9.read(),
                PmpIndex::Pmp10 => CSR::PMPADDR10.read(),
                PmpIndex::Pmp11 => CSR::PMPADDR11.read(),
                PmpIndex::Pmp12 => CSR::PMPADDR12.read(),
                PmpIndex::Pmp13 => CSR::PMPADDR13.read(),
                PmpIndex::Pmp14 => CSR::PMPADDR14.read(),
                PmpIndex::Pmp15 => CSR::PMPADDR15.read(),
            }
        }
    }

    #[inline]
    pub unsafe fn write_addr(&self, value: usize) {
        unsafe {
            match self {
                PmpIndex::Pmp0 => CSR::PMPADDR0.write(value),
                PmpIndex::Pmp1 => CSR::PMPADDR1.write(value),
                PmpIndex::Pmp2 => CSR::PMPADDR2.write(value),
                PmpIndex::Pmp3 => CSR::PMPADDR3.write(value),
                PmpIndex::Pmp4 => CSR::PMPADDR4.write(value),
                PmpIndex::Pmp5 => CSR::PMPADDR5.write(value),
                PmpIndex::Pmp6 => CSR::PMPADDR6.write(value),
                PmpIndex::Pmp7 => CSR::PMPADDR7.write(value),
                PmpIndex::Pmp8 => CSR::PMPADDR8.write(value),
                PmpIndex::Pmp9 => CSR::PMPADDR9.write(value),
                PmpIndex::Pmp10 => CSR::PMPADDR10.write(value),
                PmpIndex::Pmp11 => CSR::PMPADDR11.write(value),
                PmpIndex::Pmp12 => CSR::PMPADDR12.write(value),
                PmpIndex::Pmp13 => CSR::PMPADDR13.write(value),
                PmpIndex::Pmp14 => CSR::PMPADDR14.write(value),
                PmpIndex::Pmp15 => CSR::PMPADDR15.write(value),
            }
        }
    }
}

impl PmpConfig {
    #[inline]
    pub const fn new(
        bit_r: bool,
        bit_w: bool,
        bit_x: bool,
        addr_mode: PmpAddressMode,
        bit_l: bool,
    ) -> Self {
        let mut val = 0u8;
        if bit_r {
            val |= 0b001;
        }
        if bit_w {
            val |= 0b010;
        }
        if bit_x {
            val |= 0b100;
        }
        val |= (addr_mode as u8) << 3;
        if bit_l {
            val |= 0b1000_0000;
        }
        Self(val)
    }

    /// Returns whether the PMP entry is readable.
    #[inline]
    pub const fn is_readable(&self) -> bool {
        (self.0 & 0b001) != 0
    }

    /// Returns whether the PMP entry is writable.
    #[inline]
    pub const fn is_writable(&self) -> bool {
        (self.0 & 0b010) != 0
    }

    /// Returns whether the PMP entry is executable.
    #[inline]
    pub const fn is_executable(&self) -> bool {
        (self.0 & 0b100) != 0
    }

    /// Returns the address mode of the PMP entry.
    #[inline]
    pub const fn addr_mode(&self) -> PmpAddressMode {
        match (self.0 >> 3) & 0b11 {
            0 => PmpAddressMode::Off,
            1 => PmpAddressMode::Tor,
            2 => PmpAddressMode::Na4,
            3 => PmpAddressMode::Napot,
            _ => unreachable!(),
        }
    }

    /// Returns whether the PMP entry is locked.
    #[inline]
    pub const fn is_locked(&self) -> bool {
        (self.0 & 0b1000_0000) != 0
    }
}

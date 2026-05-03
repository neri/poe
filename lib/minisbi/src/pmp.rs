//! Physical Memory Protection (PMP) implementation for RISC-V.

use riscv::csr::CSR;

pub struct Pmp;

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
    pub const ALL: [PmpIndex; 16] = [
        PmpIndex::Pmp0,
        PmpIndex::Pmp1,
        PmpIndex::Pmp2,
        PmpIndex::Pmp3,
        PmpIndex::Pmp4,
        PmpIndex::Pmp5,
        PmpIndex::Pmp6,
        PmpIndex::Pmp7,
        PmpIndex::Pmp8,
        PmpIndex::Pmp9,
        PmpIndex::Pmp10,
        PmpIndex::Pmp11,
        PmpIndex::Pmp12,
        PmpIndex::Pmp13,
        PmpIndex::Pmp14,
        PmpIndex::Pmp15,
    ];

    #[inline]
    pub fn iter() -> impl Iterator<Item = PmpIndex> {
        Self::ALL.iter().copied()
    }

    /// Reads the PMP configuration and address for this PMP index. Returns `None` if the PMP entry is disabled.
    pub fn read(&self) -> Option<(PmpConfig, usize)> {
        let cfg = if cfg!(target_arch = "riscv32") {
            let raw_val = Pmp::read_cfg(*self as usize / 4) as u32;
            let shift = (*self as usize % 4) * 8;
            let val = (raw_val >> shift) & 0xff;
            PmpConfig(val as u8)
        } else {
            let raw_val = Pmp::read_cfg((*self as usize / 8) * 2) as u64;
            let shift = (*self as usize % 8) * 8;
            let val = (raw_val >> shift) & 0xff;
            PmpConfig(val as u8)
        };
        match cfg.addr_mode() {
            PmpAddressMode::Off => None,
            _ => {
                let addr = Pmp::read_addr(*self as usize);
                Some((cfg, addr))
            }
        }
    }

    /// Writes the PMP configuration and address for this PMP index.
    pub fn write(&self, cfg: PmpConfig, addr: usize) {
        if cfg!(target_arch = "riscv32") {
            let raw_val = Pmp::read_cfg(*self as usize / 4) as u32;
            let shift = (*self as usize % 4) * 8;
            let mask = !(0xff << shift);
            let new_val = (raw_val & mask) | ((cfg.0 as u32) << shift);
            Pmp::write_cfg(*self as usize / 4, new_val as usize);
        } else {
            todo!();
        }
        Pmp::write_addr(*self as usize, addr);
    }

    #[inline]
    pub fn write_off(&self) {
        self.write(PmpConfig(0), 0);
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

impl Pmp {
    //
    fn read_cfg(index: usize) -> usize {
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

    fn write_cfg(index: usize, value: usize) {
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

    fn read_addr(index: usize) -> usize {
        unsafe {
            match index {
                0 => CSR::PMPADDR0.read(),
                1 => CSR::PMPADDR1.read(),
                2 => CSR::PMPADDR2.read(),
                3 => CSR::PMPADDR3.read(),
                4 => CSR::PMPADDR4.read(),
                5 => CSR::PMPADDR5.read(),
                6 => CSR::PMPADDR6.read(),
                7 => CSR::PMPADDR7.read(),
                8 => CSR::PMPADDR8.read(),
                9 => CSR::PMPADDR9.read(),
                10 => CSR::PMPADDR10.read(),
                11 => CSR::PMPADDR11.read(),
                12 => CSR::PMPADDR12.read(),
                13 => CSR::PMPADDR13.read(),
                14 => CSR::PMPADDR14.read(),
                15 => CSR::PMPADDR15.read(),
                _ => unreachable!(),
            }
        }
    }

    fn write_addr(index: usize, value: usize) {
        unsafe {
            match index {
                0 => CSR::PMPADDR0.write(value),
                1 => CSR::PMPADDR1.write(value),
                2 => CSR::PMPADDR2.write(value),
                3 => CSR::PMPADDR3.write(value),
                4 => CSR::PMPADDR4.write(value),
                5 => CSR::PMPADDR5.write(value),
                6 => CSR::PMPADDR6.write(value),
                7 => CSR::PMPADDR7.write(value),
                8 => CSR::PMPADDR8.write(value),
                9 => CSR::PMPADDR9.write(value),
                10 => CSR::PMPADDR10.write(value),
                11 => CSR::PMPADDR11.write(value),
                12 => CSR::PMPADDR12.write(value),
                13 => CSR::PMPADDR13.write(value),
                14 => CSR::PMPADDR14.write(value),
                15 => CSR::PMPADDR15.write(value),
                _ => unreachable!(),
            }
        }
    }
}

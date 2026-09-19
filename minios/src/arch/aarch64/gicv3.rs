//! Arm Generic Interrupt Controller version 3 (GICv3)
//!
//! The CPU interface is accessed through the system registers (`ICC_*_EL1`).
//! If the CPU entered at EL2, `ICC_SRE_EL2` must have been enabled before dropping to EL1.

use core::arch::asm;
use core::cell::UnsafeCell;

use super::gic::Irq;

static mut GIC: UnsafeCell<GicV3> = UnsafeCell::new(GicV3::new());

/// Arm Generic Interrupt Controller version 3 (GICv3)
///
/// Only SGIs and PPIs of the current CPU are supported for now.
pub struct GicV3 {
    gicd_base: usize,
    /// SGI_base frame of the redistributor for the current CPU
    sgi_base: usize,
}

impl GicV3 {
    /// Default priority of the interrupts (same as Linux)
    const DEFAULT_PRIORITY: u8 = 0xa0;

    #[inline]
    const fn new() -> Self {
        Self {
            gicd_base: 0,
            sgi_base: 0,
        }
    }

    #[inline]
    unsafe fn shared() -> &'static mut Self {
        unsafe { (&mut *(&raw mut GIC)).get_mut() }
    }

    /// Initialize the distributor, the redistributor and the CPU interface of the current CPU.
    ///
    /// `gicr_base` and `gicr_size` describe the redistributor region.
    pub unsafe fn init(gicd_base: usize, gicr_base: usize, gicr_size: usize) -> Result<(), ()> {
        unsafe {
            let rd_base = Self::find_redistributor(gicr_base, gicr_size).ok_or(())?;

            let shared = Self::shared();
            shared.gicd_base = gicd_base;
            shared.sgi_base = rd_base + GicR::SGI_FRAME;

            // Distributor: affinity routing, enable Group 1
            shared
                .gicd(GicD::CTLR)
                .write_volatile(GicD::CTLR_ARE_NS | GicD::CTLR_ENABLE_G1A | GicD::CTLR_ENABLE_G1);
            while (shared.gicd(GicD::CTLR).read_volatile() & GicD::CTLR_RWP) != 0 {
                core::hint::spin_loop();
            }

            // Redistributor: wake up
            let waker = (rd_base + GicR::WAKER) as *mut u32;
            waker.write_volatile(waker.read_volatile() & !GicR::WAKER_PROCESSOR_SLEEP);
            while (waker.read_volatile() & GicR::WAKER_CHILDREN_ASLEEP) != 0 {
                core::hint::spin_loop();
            }

            // CPU interface: enable the system register interface
            let sre: usize;
            asm!(
                "mrs {0}, icc_sre_el1",
                "orr {0}, {0}, #1",
                "msr icc_sre_el1, {0}",
                "isb",
                "mrs {0}, icc_sre_el1",
                out(reg) sre,
            );
            if (sre & 1) == 0 {
                // Disabled by a higher exception level
                return Err(());
            }
            asm!(
                "msr icc_pmr_el1, {0}",
                "msr icc_bpr1_el1, xzr",
                "msr icc_igrpen1_el1, {1}",
                "isb",
                in(reg) 0xffusize,
                in(reg) 1usize,
            );
        }
        Ok(())
    }

    /// Find the redistributor for the current CPU by comparing GICR_TYPER with MPIDR_EL1.
    unsafe fn find_redistributor(gicr_base: usize, gicr_size: usize) -> Option<usize> {
        let mpidr: u64;
        unsafe {
            asm!("mrs {}, mpidr_el1", out(reg) mpidr);
        }
        // Aff3.Aff2.Aff1.Aff0
        let affinity = (((mpidr >> 32) & 0xff) << 24) | (mpidr & 0xff_ffff);

        let mut rd_base = gicr_base;
        while rd_base < gicr_base + gicr_size {
            let typer = unsafe { ((rd_base + GicR::TYPER) as *const u64).read_volatile() };
            if (typer >> 32) == affinity {
                return Some(rd_base);
            }
            if (typer & GicR::TYPER_LAST) != 0 {
                break;
            }
            rd_base += if (typer & GicR::TYPER_VLPIS) != 0 {
                GicR::STRIDE_V4
            } else {
                GicR::STRIDE_V3
            };
        }
        None
    }

    #[inline]
    unsafe fn gicd(&self, reg: usize) -> *mut u32 {
        (self.gicd_base + reg) as *mut u32
    }

    #[inline]
    unsafe fn sgi(&self, reg: usize) -> *mut u32 {
        (self.sgi_base + reg) as *mut u32
    }

    /// Enable the interrupt as Group 1 on the current CPU.
    pub unsafe fn enable(irq: Irq) {
        assert!(irq.0 < 32, "GICv3: SPI is not supported yet");
        unsafe {
            let shared = Self::shared();
            let bit = 1 << irq.0;

            let igroupr0 = shared.sgi(GicR::IGROUPR0);
            igroupr0.write_volatile(igroupr0.read_volatile() | bit);

            ((shared.sgi_base + GicR::IPRIORITYR + irq.0 as usize) as *mut u8)
                .write_volatile(Self::DEFAULT_PRIORITY);

            shared.sgi(GicR::ISENABLER0).write_volatile(bit);
        }
    }

    /// Acknowledge the highest priority pending Group 1 interrupt.
    ///
    /// Returns an ID of 1020 or greater if no interrupt is pending.
    #[inline]
    pub unsafe fn ack() -> Irq {
        let iar: usize;
        unsafe {
            asm!("mrs {}, icc_iar1_el1", out(reg) iar);
        }
        Irq((iar & 0xff_ffff) as u32)
    }

    #[inline]
    pub unsafe fn eoi(irq: Irq) {
        unsafe {
            asm!("msr icc_eoir1_el1, {}", in(reg) irq.0 as usize);
        }
    }
}

/// Distributor registers
struct GicD;

impl GicD {
    const CTLR: usize = 0x0000;

    const CTLR_ENABLE_G1: u32 = 1 << 0;
    const CTLR_ENABLE_G1A: u32 = 1 << 1;
    const CTLR_ARE_NS: u32 = 1 << 4;
    const CTLR_RWP: u32 = 1 << 31;
}

/// Redistributor registers
struct GicR;

impl GicR {
    // RD_base frame
    const TYPER: usize = 0x0008;
    const WAKER: usize = 0x0014;

    // SGI_base frame
    const SGI_FRAME: usize = 0x1_0000;
    const IGROUPR0: usize = 0x0080;
    const ISENABLER0: usize = 0x0100;
    const IPRIORITYR: usize = 0x0400;

    const TYPER_VLPIS: u64 = 1 << 1;
    const TYPER_LAST: u64 = 1 << 4;

    const WAKER_PROCESSOR_SLEEP: u32 = 1 << 1;
    const WAKER_CHILDREN_ASLEEP: u32 = 1 << 2;

    /// RD_base + SGI_base
    const STRIDE_V3: usize = 0x2_0000;
    /// RD_base + SGI_base + VLPI_base + reserved
    const STRIDE_V4: usize = 0x4_0000;
}

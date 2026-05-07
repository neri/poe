//! Arm Generic Interrupt Controller (GIC) driver.

use core::cell::UnsafeCell;

static mut GIC: UnsafeCell<Gic> = UnsafeCell::new(Gic::new());

pub struct Gic {
    gicc_base: usize,
    gicd_base: usize,
}

impl Gic {
    #[inline]
    const fn new() -> Self {
        Self {
            gicc_base: 0,
            gicd_base: 0,
        }
    }

    pub(crate) unsafe fn init(gicc_base: usize, gicd_base: usize) {
        unsafe {
            let shared = Self::shared();
            shared.gicc_base = gicc_base;
            shared.gicd_base = gicd_base;

            shared.gicd(GicD::CTLR).write_volatile(0b0001);

            shared.gicc(GicC::PMR).write_volatile(0xFF);
            shared.gicc(GicC::CTLR).write_volatile(0b0001);
        }
    }

    #[inline]
    unsafe fn shared() -> &'static mut Self {
        unsafe { (&mut *(&raw mut GIC)).get_mut() }
    }

    #[inline]
    unsafe fn gicc(&self, reg: GicC) -> *mut u32 {
        (self.gicc_base + reg as usize) as *mut u32
    }

    #[inline]
    unsafe fn gicd(&self, reg: GicD) -> *mut u32 {
        (self.gicd_base + reg as usize) as *mut u32
    }

    #[inline]
    pub unsafe fn enable(irq: Irq) {
        unsafe {
            let shared = Self::shared();
            let reg_index = irq.0 / 32;
            let bit_index = irq.0 % 32;
            shared
                .gicd(GicD::ISENABLER)
                .add(reg_index as usize)
                .write_volatile(1 << bit_index);
        }
    }

    #[inline]
    pub unsafe fn eoi(irq: Irq) {
        unsafe {
            let shared = Self::shared();
            shared.gicc(GicC::EOIR).write_volatile(irq.0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Irq(pub u32);

#[allow(dead_code)]
pub enum GicC {
    CTLR = 0x0000,
    PMR = 0x0004,
    BPR = 0x0008,
    IAR = 0x000C,
    EOIR = 0x0010,
}

#[allow(dead_code)]
pub enum GicD {
    CTLR = 0x0000,
    TYPER = 0x0004,
    IIDR = 0x0008,
    IGROUPR = 0x0080,
    ISENABLER = 0x0100,
    ICENABLER = 0x0180,
    ISPENDR = 0x0200,
    ICPENDR = 0x0280,
    ISACTIVER = 0x0300,
    ICACTIVER = 0x0380,
    IPRIORITYR = 0x0400,
    ITARGETSR = 0x0800,
    ICFGR = 0x0C00,
}

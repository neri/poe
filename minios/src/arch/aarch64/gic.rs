//! Arm Generic Interrupt Controller (GIC)

use core::cell::UnsafeCell;

static mut GIC: UnsafeCell<Gic> = UnsafeCell::new(Gic::new());

/// Common IRQ for ARM Generic Timer (Physical)
pub const IRQ_CNTHP: Irq = Irq(26);
/// Common IRQ for ARM Generic Timer (Virtual)
pub const IRQ_CNTV: Irq = Irq(27);
/// Common IRQ for ARM Generic Timer (Physical Non-Secure)
pub const IRQ_CNTPNS: Irq = Irq(29);
/// Common IRQ for ARM Generic Timer (Physical Secure)
pub const IRQ_CNTPS: Irq = Irq(30);

/// Arm Generic Interrupt Controller (GIC)
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
    /// CPU Interface Control Register
    CTLR = 0x0000,
    /// Interrupt Priority Mask Register
    PMR = 0x0004,
    /// Binary Point Register
    BPR = 0x0008,
    /// Interrupt Acknowledge Register
    IAR = 0x000C,
    /// End of Interrupt Register
    EOIR = 0x0010,
    /// Running Priority Register
    RPR = 0x0014,
    /// Highest Priority Pending Interrupt Register
    HPPIR = 0x0018,
    /// Aliased Binary Point Register
    ABPR = 0x001C,
    /// Aliased Interrupt Acknowledge Register
    AIAR = 0x0020,
    /// Aliased End of Interrupt Register
    AEOIR = 0x0024,
    /// Aliased Highest Priority Pending Interrupt Register
    AHPPIR = 0x0028,
    /// Status Register
    STATUSR = 0x002c,
}

#[allow(dead_code)]
pub enum GicD {
    /// Distributor Control Register
    CTLR = 0x0000,
    /// Interrupt Controller Type Register
    TYPER = 0x0004,
    /// Implementer Identification Register
    IIDR = 0x0008,
    /// Interrupt Distributor Type Register 2
    TYPER2 = 0x000c,
    /// Error Reporting Status Register
    STATUSR = 0x0010,
    /// Interrupt Group Register
    IGROUPR = 0x0080,
    /// Interrupt Set-Enable Register
    ISENABLER = 0x0100,
    /// Interrupt Clear-Enable Register
    ICENABLER = 0x0180,
    ISPENDR = 0x0200,
    ICPENDR = 0x0280,
    ISACTIVER = 0x0300,
    ICACTIVER = 0x0380,
    IPRIORITYR = 0x0400,
    ITARGETSR = 0x0800,
    ICFGR = 0x0C00,
}

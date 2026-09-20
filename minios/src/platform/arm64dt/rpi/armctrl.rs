//! BCM2836 ARMCTRL interrupt controller (GPU interrupt cascade).

use crate::arch::gic::{IRQ_SPURIOUS, Irq};

static mut BASE: usize = 0;
/// What this driver put on the cascade, per bank. The pending registers are
/// not guaranteed to be filtered by the enable registers, so an interrupt
/// nobody asked for must not be reported as ours.
static mut ENABLED: [u32; 3] = [0; 3];

pub struct Armctrl;
impl Armctrl {
    pub const COMPATIBLE: &str = "brcm,bcm2836-armctrl-ic";
    // The DT node starts at the interrupt-controller register block
    // (0x...b200), not at the peripheral window base.
    const PENDING_BASIC: usize = 0x00;
    const PENDING_1: usize = 0x04;
    const PENDING_2: usize = 0x08;
    const ENABLE_1: usize = 0x10;
    const ENABLE_2: usize = 0x14;
    const ENABLE_BASIC: usize = 0x18;
    const DISABLE_1: usize = 0x1c;
    const DISABLE_2: usize = 0x20;
    const DISABLE_BASIC: usize = 0x24;
    /// GPU IRQs that the basic pending register also surfaces, as bits 10..20.
    const SHORTCUTS: [u32; 11] = [7, 9, 10, 18, 19, 53, 54, 55, 56, 57, 62];
    pub unsafe fn init(base: usize) {
        unsafe { BASE = base }
    }
    pub fn is_initialized() -> bool {
        unsafe { BASE != 0 }
    }
    pub unsafe fn enable(irq: Irq) {
        unsafe {
            let Some((bank, bit)) = Self::bank(irq) else {
                return;
            };
            ENABLED[bank] |= 1 << bit;
            Self::reg([Self::ENABLE_1, Self::ENABLE_2, Self::ENABLE_BASIC][bank])
                .write_volatile(1 << bit)
        }
    }
    /// Takes an interrupt off the cascade. The registers are write-1-to-clear
    /// per bit, so this does not disturb the other sources.
    pub unsafe fn disable(irq: Irq) {
        unsafe {
            let Some((bank, bit)) = Self::bank(irq) else {
                return;
            };
            ENABLED[bank] &= !(1 << bit);
            Self::reg([Self::DISABLE_1, Self::DISABLE_2, Self::DISABLE_BASIC][bank])
                .write_volatile(1 << bit)
        }
    }
    pub unsafe fn ack() -> Irq {
        unsafe {
            // Bits 8 and 9 of the basic register report pending register 1/2
            // sources *other than* the ones already surfaced as the shortcut
            // bits 10..20, and USB (GPU IRQ 9) is one of those shortcuts. With
            // only USB pending, neither summary bit is set, so trusting them
            // reports no interrupt at all and the level-triggered cascade
            // re-enters forever. Read the bank registers directly.
            let pending = Self::reg(Self::PENDING_1).read_volatile() & ENABLED[0];
            if pending != 0 {
                return Irq(pending.trailing_zeros());
            }
            let pending = Self::reg(Self::PENDING_2).read_volatile() & ENABLED[1];
            if pending != 0 {
                return Irq(32 + pending.trailing_zeros());
            }
            let basic = Self::reg(Self::PENDING_BASIC).read_volatile();
            let direct = basic & 0xff & ENABLED[2];
            if direct != 0 {
                return Irq(64 + direct.trailing_zeros());
            }
            // Only if a core reports one of those GPU IRQs in the basic
            // register alone.
            let mut shortcuts = (basic >> 10) & 0x7ff;
            while shortcuts != 0 {
                let index = shortcuts.trailing_zeros();
                let irq = Irq(Self::SHORTCUTS[index as usize]);
                if let Some((bank, bit)) = Self::bank(irq)
                    && ENABLED[bank] & (1 << bit) != 0
                {
                    return irq;
                }
                shortcuts &= !(1 << index);
            }
            IRQ_SPURIOUS
        }
    }
    /// Splits an IRQ number into its register bank and bit.
    const fn bank(irq: Irq) -> Option<(usize, u32)> {
        match irq.0 {
            0..=31 => Some((0, irq.0)),
            32..=63 => Some((1, irq.0 - 32)),
            64..=71 => Some((2, irq.0 - 64)),
            _ => None,
        }
    }
    unsafe fn reg(offset: usize) -> *mut u32 {
        unsafe { (BASE + offset) as *mut u32 }
    }
}

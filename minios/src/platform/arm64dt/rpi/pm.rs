//! Power management and watchdog of BCM2835 and later (Raspberry Pi 3 / 4)
//!
//! Only the full reset through the watchdog is supported (the same as Linux `bcm2835_restart`).

static mut BASE: usize = 0;

pub struct Pm;

impl Pm {
    pub const COMPATIBLE: &str = "brcm,bcm2835-pm-wdt";

    /// Offset of PM from the peripheral base, used if the device tree does not have it
    pub const DEFAULT_OFFSET: usize = 0x0010_0000;

    const PM_RSTC: usize = 0x1c;
    const PM_RSTS: usize = 0x20;
    const PM_WDOG: usize = 0x24;

    /// Written to the upper 8 bits of every write
    const PASSWORD: u32 = 0x5a00_0000;
    const PASSWORD_MASK: u32 = 0xff00_0000;
    const RSTC_WRCFG_CLR: u32 = 0xffff_ffcf;
    const RSTC_WRCFG_FULL_RESET: u32 = 0x0000_0020;
    /// Clears the boot partition (bits 0, 2, 4, 6, 8, 10), so that the firmware boots normally
    const RSTS_PARTITION_CLR: u32 = 0xffff_faaa;
    /// Watchdog timeout in ticks (about 16µs each)
    const WDOG_TICKS: u32 = 10;

    pub unsafe fn init(base: usize) {
        unsafe {
            BASE = base;
        }
    }

    /// Resets the system with the watchdog. Returns if PM is not initialized.
    pub unsafe fn reset_system() {
        unsafe {
            if BASE == 0 {
                return;
            }
            let rsts = Self::reg(Self::PM_RSTS);
            rsts.write_volatile(
                Self::PASSWORD
                    | (rsts.read_volatile() & Self::RSTS_PARTITION_CLR & !Self::PASSWORD_MASK),
            );

            Self::reg(Self::PM_WDOG).write_volatile(Self::PASSWORD | Self::WDOG_TICKS);

            let rstc = Self::reg(Self::PM_RSTC);
            rstc.write_volatile(
                Self::PASSWORD
                    | (rstc.read_volatile() & Self::RSTC_WRCFG_CLR & !Self::PASSWORD_MASK)
                    | Self::RSTC_WRCFG_FULL_RESET,
            );
        }
    }

    #[inline]
    unsafe fn reg(offset: usize) -> *mut u32 {
        unsafe { (BASE + offset) as *mut u32 }
    }
}

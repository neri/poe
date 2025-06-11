//! Wrapper library for RISC-V Supervisor Binary Interface
#![cfg_attr(not(test), no_std)]

use core::arch::asm;
use minilib::unknown_enum;
use minilib::unknown_enum::*;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SbiRet {
    error: Unknown<SbiError, isize>,
    value: usize,
}

impl SbiRet {
    #[inline]
    fn new(a0: isize, a1: usize) -> Self {
        Self {
            error: Unknown::unknown(a0),
            value: a1,
        }
    }
}

unknown_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum SbiError (isize) {
        /// Completed successfully. (No error)
        Success = 0,
        /// Failed
        Failed = -1,
        /// Not supported
        NotSupported = -2,
        /// Invalid parameter(s)
        InvalidParam = -3,
        /// Denied or not allowed
        Denied = -4,
        /// Invalid address(s)
        InvalidAddress = -5,
        /// Already available
        AlreadyAvailable = -6,
        /// Already started
        AlreadtStarted = -7,
        /// Already stopped
        AlreadyStopped = -8,
        /// Shared memory not available
        NoShmem = -9,
        /// Invalid state
        InvalidState = -10,
        /// Invalid range
        InvalidRange = -11,
        /// Failed due to timeout
        TimeOut = -12,
        /// I/O error
        Io = -13,
    }
}

/// SBI extension ID
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Eid(pub usize);

impl Eid {
    /// `long sbi_set_timer(uint64_t stime_value)`
    pub const SET_TIMER: Self = Self(0);
    /// `long sbi_console_putchar(int ch)`
    pub const CONSOLE_PUTCHAR: Self = Self(1);
    /// `long sbi_console_getchar(void)`
    pub const CONSOLE_GETCHAR: Self = Self(2);
    /// `long sbi_clear_ipi(void)`
    pub const CLEAR_IPI: Self = Self(3);
    /// `long sbi_send_ipi(const unsigned long *hart_mask)`
    pub const SEND_IPI: Self = Self(4);
    /// `long sbi_remote_fence_i(const unsigned long *hart_mask)`
    pub const REMOTE_FENCE_I: Self = Self(5);
    /// ```C
    /// long sbi_remote_sfence_vma(const unsigned long *hart_mask,
    ///     unsigned long start,
    ///     unsigned long size)
    /// ```
    pub const REMOTE_SFENCE_VMA: Self = Self(6);
    /// ```C
    /// long sbi_remote_sfence_vma_asid(const unsigned long *hart_mask,
    ///     unsigned long start,
    ///     unsigned long size,
    ///     unsigned long asid)
    /// ```
    pub const REMOTE_SFENCE_VMA_ASID: Self = Self(7);
    /// `void sbi_shutdown(void)`
    pub const SHUTDOWN: Self = Self(8);

    /// Base Extension (EID #0x10)
    pub const BASE: Self = Self(0x10);

    /// Timer Extension (EID #0x54494D45 "TIME")
    pub const TIME: Self = Self(0x54494D45);

    /// IPI Extension (EID #0x735049 "sPI: s-mode IPI")
    pub const IPI: Self = Self(0x735049);

    /// RFENCE Extension (EID #0x52464E43 "RFNC")
    pub const RFENCE: Self = Self(0x52464E43);

    /// Hart State Management Extension (EID #0x48534D "HSM")
    pub const HSM: Self = Self(0x48534D);

    /// System Reset Extension (EID #0x53525354 "SRST")
    pub const SYSTEM_RESET: Self = Self(0x53525354);

    /// Performance Monitoring Unit Extension (EID #0x504D55 "PMU")
    pub const PMU: Self = Self(0x504D55);

    /// Debug Console Extension (EID #0x4442434E "DBCN")
    pub const DEBUG_CONSOLE: Self = Self(0x4442434E);

    /// System Suspend Extension (EID #0x53555350 "SUSP")
    pub const SYSTEM_SUSPEND: Self = Self(0x53555350);

    /// CPPC Extension (EID #0x43505043 "CPPC")
    pub const CPPC: Self = Self(0x43505043);

    /// Nested Acceleration Extension (EID #0x4E41434C "NACL")
    pub const NACL: Self = Self(0x4E41434C);

    /// Steal-time Accounting Extension (EID #0x535441 "STA")
    pub const STEAL_TIME_ACCOUNTING: Self = Self(0x535441);

    /// Supervisor Software Events Extension (EID #0x535345 "SSE")
    pub const SUPERVISOR_SOFTWARE_EVENTS: Self = Self(0x535345);

    /// SBI Firmware Features Extension (EID #0x46574654 "FWFT")
    pub const FIRMWARE_FEATURES: Self = Self(0x46574654);

    /// Debug Triggers Extension (EID #0x44425452 "DBTR")
    pub const DEBUG_TRIGGERS: Self = Self(0x44425452);

    /// Message Proxy Extension (EID #0x4D505859 “MPXY”)
    pub const MESSAGE_PROXY: Self = Self(0x4D505859);

    /// Experimental SBI Extension Space (EIDs #0x08000000 - #0x08FFFFFF)
    pub const EXPERIMENTAL_BASE: Self = Self(0x08000000);

    /// Experimental SBI Extension Space (EIDs #0x08000000 - #0x08FFFFFF)
    #[inline]
    pub const fn experimental(n: usize) -> Self {
        Self(Self::EXPERIMENTAL_BASE.0 + n)
    }

    /// Vendor-Specific SBI Extension Space (EIDs #0x09000000 - #0x09FFFFFF)
    pub const VENDOR_SPECIFIC_BASE: Self = Self(0x09000000);

    /// Vendor-Specific SBI Extension Space (EIDs #0x09000000 - #0x09FFFFFF)
    #[inline]
    pub const fn vendor(n: usize) -> Self {
        Self(Self::VENDOR_SPECIFIC_BASE.0 + n)
    }

    /// Firmware Specific SBI Extension Space (EIDs #0x0A000000 - #0x0AFFFFFF)
    pub const FIRMWARE_SPECIFIC_BASE: Self = Self(0x0A000000);

    /// Firmware Specific SBI Extension Space (EIDs #0x0A000000 - #0x0AFFFFFF)
    #[inline]
    pub const fn firmware(n: usize) -> Self {
        Self(Self::FIRMWARE_SPECIFIC_BASE.0 + n)
    }
}

/// SBI function ID
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fid(pub usize);

impl Fid {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EidFid {
    pub eid: Eid,
    pub fid: Fid,
}

impl EidFid {
    #[inline]
    pub const fn new(eid: Eid, fid: Fid) -> Self {
        Self { eid, fid }
    }
}

impl EidFid {
    /// `struct sbiret sbi_get_spec_version(void)`
    pub const GET_SPEC_VERSION: Self = Self::new(Eid::BASE, Fid(0));
    /// `struct sbiret sbi_get_impl_id(void)`
    pub const GET_IMPL_ID: Self = Self::new(Eid::BASE, Fid(1));
    /// `struct sbiret sbi_get_impl_version(void)`
    pub const GET_IMPL_VERSION: Self = Self::new(Eid::BASE, Fid(2));
    /// `struct sbiret sbi_probe_extension(long extension_id)`
    pub const PROBE_EXTENSION: Self = Self::new(Eid::BASE, Fid(3));
    /// `struct sbiret sbi_get_mvendorid(void)`
    pub const GET_MVENDORID: Self = Self::new(Eid::BASE, Fid(4));
    /// `struct sbiret sbi_get_marchid(void)`
    pub const GET_MARCHID: Self = Self::new(Eid::BASE, Fid(5));
    /// `struct sbiret sbi_get_mimpid(void)`
    pub const GET_MIMPID: Self = Self::new(Eid::BASE, Fid(6));

    /// `struct sbiret sbi_set_timer(uint64_t stime_value)`
    pub const SET_TIMER: Self = Self::new(Eid::TIME, Fid(0));

    ///  ```C
    /// struct sbiret sbi_send_ipi(unsigned long hart_mask,
    ///     unsigned long hart_mask_base)
    /// ```
    pub const SEND_IPI: Self = Self::new(Eid::IPI, Fid(0));

    /// `struct sbiret sbi_system_reset(uint32_t reset_type, uint32_t reset_reason)`
    pub const SYSTENM_RESET: Self = Self::new(Eid::SYSTEM_RESET, Fid(0));
}

unknown_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum ImplementationID (usize) {
        /// Berkeley Boot Loader (BBL)
        BBL = 0,
        OpenSBI = 1,
        Xvisor = 2,
        KVM = 3,
        RustSBI = 4,
        Diosix = 5,
        Coffer = 6,
        /// Xen Project
        Xen = 7,
        /// PolarFire Hart Software Services
        PolarFire = 8,
        Coreboot = 9,
        Oreboot = 10,
        Bhyve = 11,
    }
}

#[macro_export]
macro_rules! call_sbi {
    ($eid_fid:expr) => {
        _call_sbi($eid_fid, &[])
    };
    ($eid_fid:expr, $a0:expr) => {
        _call_sbi($eid_fid, &[$a0])
    };
    ($eid_fid:expr, $a0:expr, $a1:expr) => {
        _call_sbi($eid_fid, &[$a0, $a1])
    };
    ($eid_fid:expr, $a0:expr, $a1:expr, $a2:expr) => {
        _call_sbi($eid_fid, &[$a0, $a1, $a2])
    };
    ($eid_fid:expr, $a0:expr, $a1:expr, $a2:expr, $a3:expr) => {
        _call_sbi($eid_fid, &[$a0, $a1, $a2, $a3])
    };
    ($eid_fid:expr, $a0:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr) => {
        _call_sbi($eid_fid, &[$a0, $a1, $a2, $a3, $a4])
    };
    ($eid_fid:expr, $a0:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr, $a5:expr) => {
        _call_sbi($eid_fid, &[$a0, $a1, $a2, $a3, $a4, $a5])
    };
}

pub struct HartMask<'a>(pub &'a [usize]);

#[inline]
pub unsafe fn _call_sbi(eid_fid: EidFid, args: &[usize]) -> Result<SbiRet, SbiRet> {
    let result = unsafe {
        match args.len() {
            0 => {
                let a0: isize;
                let a1: usize;
                asm!("ecall",
                    in("a7") eid_fid.eid.0,
                    in("a6") eid_fid.fid.0,
                    lateout("a0") a0,
                    lateout("a1") a1,
                );
                SbiRet::new(a0, a1)
            }
            1 => {
                let a0: isize;
                let a1: usize;
                asm!("ecall",
                    in("a7") eid_fid.eid.0,
                    in("a6") eid_fid.fid.0,
                    in("a0") args[0],
                    lateout("a0") a0,
                    lateout("a1") a1,
                );
                SbiRet::new(a0, a1)
            }
            2 => {
                let a0: isize;
                let a1: usize;
                asm!("ecall",
                    in("a7") eid_fid.eid.0,
                    in("a6") eid_fid.fid.0,
                    in("a0") args[0],
                    in("a1") args[1],
                    lateout("a0") a0,
                    lateout("a1") a1,
                );
                SbiRet::new(a0, a1)
            }
            3 => {
                let a0: isize;
                let a1: usize;
                asm!("ecall",
                    in("a7") eid_fid.eid.0,
                    in("a6") eid_fid.fid.0,
                    in("a0") args[0],
                    in("a1") args[1],
                    in("a2") args[2],
                    lateout("a0") a0,
                    lateout("a1") a1,
                );
                SbiRet::new(a0, a1)
            }
            4 => {
                let a0: isize;
                let a1: usize;
                asm!("ecall",
                    in("a7") eid_fid.eid.0,
                    in("a6") eid_fid.fid.0,
                    in("a0") args[0],
                    in("a1") args[1],
                    in("a2") args[2],
                    in("a3") args[3],
                    lateout("a0") a0,
                    lateout("a1") a1,
                );
                SbiRet::new(a0, a1)
            }
            5 => {
                let a0: isize;
                let a1: usize;
                asm!("ecall",
                    in("a7") eid_fid.eid.0,
                    in("a6") eid_fid.fid.0,
                    in("a0") args[0],
                    in("a1") args[1],
                    in("a2") args[2],
                    in("a3") args[3],
                    in("a4") args[4],
                    lateout("a0") a0,
                    lateout("a1") a1,
                );
                SbiRet::new(a0, a1)
            }
            6 => {
                let a0: isize;
                let a1: usize;
                asm!("ecall",
                    in("a7") eid_fid.eid.0,
                    in("a6") eid_fid.fid.0,
                    in("a0") args[0],
                    in("a1") args[1],
                    in("a2") args[2],
                    in("a3") args[3],
                    in("a4") args[4],
                    in("a5") args[5],
                    lateout("a0") a0,
                    lateout("a1") a1,
                );
                SbiRet::new(a0, a1)
            }
            _ => todo!(),
        }
    };
    if matches!(result.error.known_value(), Ok(SbiError::Success)) {
        Ok(result)
    } else {
        Err(result)
    }
}

pub mod legacy {
    use super::*;

    /// Programs the clock for next event after `stime_value` time. This function also clears the pending timer interrupt bit.
    ///
    /// If the supervisor wishes to clear the timer interrupt without scheduling the next timer event, it can either request a timer interrupt infinitely far into the future (i.e., (uint64_t)-1), or it can instead mask the timer interrupt by clearing `sie.STIE` CSR bit.
    ///
    /// This SBI call returns 0 upon success or an implementation specific negative error code.
    #[inline]
    #[doc(alias = "sbi_set_timer")]
    pub fn set_timer(stime_value: u64) {
        #[cfg(target_arch = "riscv32")]
        unsafe {
            asm!("ecall",
                in("a7") Eid::SET_TIMER.0,
                in("a0") stime_value as usize,
                in("a1") (stime_value >> 32) as usize,
                lateout("a0") _,
            );
        }

        #[cfg(target_arch = "riscv64")]
        unsafe {
            asm!("ecall",
                in("a7") Eid::SET_TIMER.0,
                in("a0") stime_value as usize,
                lateout("a0") _,
            );
        }
    }

    /// Write data present in ch to debug console.
    ///
    /// Unlike `sbi_console_getchar()`, this SBI call **will block** if there remain any pending characters to be transmitted or if the receiving terminal is not yet ready to receive the byte. However, if the console doesn’t exist at all, then the character is thrown away.
    ///
    /// This SBI call returns 0 upon success or an implementation specific negative error code.
    #[inline]
    #[doc(alias = "sbi_putchar")]
    pub fn putchar(ch: u8) {
        unsafe {
            asm!("ecall",
                in("a7") Eid::CONSOLE_PUTCHAR.0,
                in("a0") ch as usize,
                lateout("a0") _,
            );
        }
    }

    pub struct StdOut;

    impl core::fmt::Write for StdOut {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for c in s.bytes() {
                putchar(c);
            }
            Ok(())
        }
    }

    /// Read a byte from debug console.
    ///
    /// The SBI call returns the byte on success, or -1 for failure.
    #[inline]
    #[doc(alias = "sbi_getchar")]
    pub fn getchar() -> Option<u8> {
        unsafe {
            let c: isize;
            asm!("ecall",
                in("a7") Eid::CONSOLE_GETCHAR.0,
                lateout("a0") c,
            );
            match c {
                0..=255 => Some(c as u8),
                _ => None,
            }
        }
    }

    /// Clears the pending IPIs if any. The IPI is cleared only in the hart for which this SBI call is invoked. `sbi_clear_ipi()` is deprecated because S-mode code can clear sip.SSIP CSR bit directly.
    ///
    /// This SBI call returns 0 if no IPI had been pending, or an implementation specific positive value if an IPI had been pending.
    #[deprecated = "because S-mode code can clear sip.SSIP CSR bit directly."]
    #[inline]
    #[doc(alias = "sbi_clear_ipi")]
    pub fn clear_ipi() -> Result<(), isize> {
        unsafe {
            let result: isize;
            asm!("ecall",
                in("a7")Eid::CLEAR_IPI.0,
                lateout("a0") result,
            );
            match result {
                0 => Ok(()),
                err => Err(err),
            }
        }
    }

    /// Send an inter-processor interrupt to all the harts defined in `hart_mask`. Interprocessor interrupts manifest at the receiving harts as Supervisor Software Interrupts.
    ///
    /// `hart_mask` is a virtual address that points to a bit-vector of harts. The bit vector is represented as a sequence of unsigned longs whose length equals the number of harts in the system divided by the number of bits in an unsigned long, rounded up to the next integer.
    ///
    /// This SBI call returns 0 upon success or an implementation specific negative error code.
    #[inline]
    #[doc(alias = "sbi_send_ipi")]
    pub fn send_ipi(hart_mask: &HartMask) -> Result<(), isize> {
        unsafe {
            let result: isize;
            asm!("ecall",
                in("a7") Eid::SEND_IPI.0,
                in("a0") hart_mask.0.as_ptr() as usize,
                lateout("a0") result,
            );
            match result {
                0 => Ok(()),
                err => Err(err),
            }
        }
    }

    /// Instructs remote harts to execute `FENCE.I` instruction. The `hart_mask` is same as described in `sbi_send_ipi()`.
    ///
    /// This SBI call returns 0 upon success or an implementation specific negative error code.
    #[inline]
    #[doc(alias = "sbi_remote_fence_i")]
    pub fn remote_fence_i(hart_mask: &HartMask) -> Result<(), isize> {
        unsafe {
            let result: isize;
            asm!("ecall",
                in("a7") Eid::REMOTE_FENCE_I.0,
                in("a0") hart_mask.0.as_ptr() as usize,
                lateout("a0") result,
            );
            match result {
                0 => Ok(()),
                err => Err(err),
            }
        }
    }

    /// Instructs the remote harts to execute one or more `SFENCE.VMA` instructions, covering the range of virtual addresses between start and size.
    ///
    /// This SBI call returns 0 upon success or an implementation specific negative error code.
    #[inline]
    #[doc(alias = "sbi_remote_sfence_vma")]
    pub fn remote_sfence_vma(hart_mask: &HartMask, start: usize, size: usize) -> Result<(), isize> {
        unsafe {
            let result: isize;
            asm!("ecall",
                in("a7") Eid::REMOTE_SFENCE_VMA.0,
                in("a0") hart_mask.0.as_ptr() as usize,
                in("a1") start,
                in("a2") size,
                lateout("a0") result,
            );
            match result {
                0 => Ok(()),
                err => Err(err),
            }
        }
    }

    /// Instruct the remote harts to execute one or more `SFENCE.VMA` instructions, covering the range of virtual addresses between start and size. This covers only the given `ASID`.
    ///
    /// This SBI call returns 0 upon success or an implementation specific negative error code.
    #[inline]
    #[doc(alias = "sbi_remote_sfence_vma_asid")]
    pub fn remote_sfence_vma_asid(
        hart_mask: &HartMask,
        start: usize,
        size: usize,
        asid: usize,
    ) -> Result<(), isize> {
        unsafe {
            let result: isize;
            asm!("ecall",
                in("a7") Eid::REMOTE_SFENCE_VMA_ASID.0,
                in("a0") hart_mask.0.as_ptr() as usize,
                in("a1") start,
                in("a2") size,
                in("a3") asid,
                lateout("a0") result,
            );
            match result {
                0 => Ok(()),
                err => Err(err),
            }
        }
    }

    /// Puts all the harts to shutdown state from supervisor point of view.
    ///
    /// This SBI call doesn’t return irrespective whether it succeeds or fails.
    #[inline]
    #[doc(alias = "sbi_shutdown")]
    pub fn shutdown() -> ! {
        unsafe {
            asm!(
                "ecall",
                in("a7") Eid::SHUTDOWN.0,
                options(noreturn)
            );
        }
    }
}

pub mod base {
    use super::*;

    /// Get the SBI specification version implemented by the SBI implementation.
    ///
    /// This SBI call returns the SBI specification version implemented by the SBI implementation.
    #[inline]
    #[doc(alias = "sbi_get_spec_version")]
    pub fn get_spec_version() -> SpecVersion {
        unsafe {
            let result: u32;
            asm!("ecall",
                in("a7") EidFid::GET_SPEC_VERSION.eid.0,
                in("a6") EidFid::GET_SPEC_VERSION.fid.0,
                lateout("a0") _,
                lateout("a1") result,
            );
            SpecVersion(result as u32)
        }
    }

    pub struct SpecVersion(pub u32);

    impl SpecVersion {
        #[inline]
        pub const fn major(&self) -> u32 {
            (self.0 >> 24) & 0x7f
        }

        #[inline]
        pub const fn minor(&self) -> u32 {
            self.0 & 0xffffff
        }
    }

    /// Get the SBI implementation ID.
    ///
    /// This SBI call returns the SBI implementation ID.
    #[inline]
    #[doc(alias = "sbi_get_impl_id")]
    pub fn get_impl_id() -> Result<Unknown<ImplementationID, usize>, Unknown<SbiError, isize>> {
        unsafe {
            call_sbi!(EidFid::GET_IMPL_ID)
                .map(|v| Unknown::unknown(v.value))
                .map_err(|v| v.error)
        }
    }

    /// Get the SBI implementation version.
    ///
    /// This SBI call returns the SBI implementation version.
    #[inline]
    #[doc(alias = "sbi_get_impl_version")]
    pub fn get_impl_version() -> Result<usize, Unknown<SbiError, isize>> {
        unsafe {
            call_sbi!(EidFid::GET_IMPL_VERSION)
                .map(|v| v.value)
                .map_err(|v| v.error)
        }
    }
}

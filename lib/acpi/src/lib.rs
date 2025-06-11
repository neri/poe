//! Advanced Configuration and Power Interface (ACPI)
#![cfg_attr(not(test), no_std)]

mod tables;
pub use tables::*;
pub mod bgrt;
pub mod dsdt;
pub mod fadt;
pub mod hpet;
pub mod madt;

use core::ffi::c_void;

#[cfg(feature = "uuid")]
use uuid::{Guid, guid};

/// EFI GUID of the old ACPI 1 RSDP
#[cfg(feature = "uuid")]
pub const ACPI_10_TABLE_GUID: Guid = guid!("eb9d2d30-2d88-11d3-9a16-0090273fc14d");

/// EFI GUID of the ACPI 2 RSDP
#[cfg(feature = "uuid")]
pub const ACPI_20_TABLE_GUID: Guid = guid!("8868e871-e4f1-11d3-bc22-0080c73c8881");

/// Root System Description Pointer
#[repr(C, packed)]
#[allow(unused)]
pub struct RsdPtr {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    rev: u8,
    rsdt_addr: u32,
    len: u32,
    xsdt_addr: u64,
    checksum2: u8,
    _reserved: [u8; 3],
}

impl RsdPtr {
    pub const VALID_SIGNATURE: [u8; 8] = *b"RSD PTR ";
    pub const CURRENT_REV: u8 = 2;

    #[inline]
    pub unsafe fn parse_lite(ptr: *const c_void) -> Option<&'static Self> {
        let p = unsafe { &*(ptr as *const Self) };
        p.is_valid().then(|| p)
    }

    #[inline]
    pub unsafe fn parse_extended(ptr: *const c_void) -> Option<&'static Self> {
        unsafe {
            let _ = RsdPtrOld::parse(ptr)?;

            let p = &*(ptr as *const Self);
            p.is_valid().then(|| ())?;

            let q = ptr as *const u8;
            let mut sum: u8 = 0;
            for i in 0..p.len as usize {
                sum = sum.wrapping_add(q.add(i).read_volatile());
            }
            (sum == 0).then(|| p)
        }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.signature == Self::VALID_SIGNATURE && self.rev >= Self::CURRENT_REV
    }

    #[inline]
    pub const fn rev(&self) -> u8 {
        self.rev
    }

    #[inline]
    pub fn xsdt(&self) -> &Xsdt {
        unsafe { &*(self.xsdt_addr as usize as *const Xsdt) }
    }
}

/// Root System Description Pointer revision 0 (ACPI version 1)
#[repr(C, packed)]
#[allow(unused)]
pub struct RsdPtrOld {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    rev: u8,
    rsdt_addr: u32,
}

impl RsdPtrOld {
    pub const VALID_SIGNATURE: [u8; 8] = *b"RSD PTR ";
    pub const CURRENT_REV: u8 = 0;

    #[inline]
    pub unsafe fn parse(ptr: *const c_void) -> Option<&'static Self> {
        let p = unsafe { &*(ptr as *const Self) };
        p.is_valid().then(|| p)
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        if self.signature != Self::VALID_SIGNATURE {
            return false;
        }

        let q = self as *const _ as *const u8;
        let mut sum: u8 = 0;
        for i in 0..20 {
            sum = sum.wrapping_add(unsafe { q.add(i).read_volatile() });
        }
        sum == 0
    }

    #[inline]
    pub const fn rev(&self) -> u8 {
        self.rev
    }

    #[inline]
    pub fn rsdt(&self) -> &Rsdt {
        unsafe { &*(self.rsdt_addr as usize as *const Rsdt) }
    }
}

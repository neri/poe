//! coreboot table
//!
//! On Chromebooks, depthcharge adds `/firmware/coreboot` to the device tree.
//! Its first `reg` entry points to the coreboot table.
//!
//! The table may be only 4-byte aligned and the MMU may be off (Device memory),
//! so it is read with aligned 32-bit and 8-bit accesses only.

use fdt::{NodeName, PropName};

use crate::platform::arm64dt::fb::Framebuffer;
use crate::*;

const SIGNATURE: u32 = u32::from_le_bytes(*b"LBIO");

const HEADER_SIZE: usize = 24;
const MAX_TABLE_SIZE: usize = 0x10_0000;

const LB_TAG_FORWARD: u32 = 0x0011;
const LB_TAG_FRAMEBUFFER: u32 = 0x0012;
const LB_TAG_VPD: u32 = 0x002c;

/// `struct vpd_cbmem`: magic "CROS", version, ro_size, rw_size, then the RO and RW data
const VPD_CBMEM_MAGIC: u32 = 0x4352_4f53;
const VPD_CBMEM_VERSION: u32 = 1;
const VPD_CBMEM_HEADER_SIZE: usize = 16;
const MAX_VPD_SIZE: usize = 0x1_0000;

/// Returns the framebuffer described in the coreboot table, if any.
///
/// The address may be 0: on Rockchip, coreboot leaves it to the payload
/// (libpayload allocates the framebuffer and keeps the address only in its own copy),
/// so the caller has to find it elsewhere. The result is not validated.
pub fn find_framebuffer(dt: &fdt::DeviceTree) -> Option<Framebuffer> {
    let table = find_table(dt)?;
    unsafe {
        find_entry(table, LB_TAG_FRAMEBUFFER, true).and_then(|entry| parse_framebuffer(entry))
    }
}

/// Returns a copy of the RO VPD (Vital Product Data), which coreboot copies into CBMEM.
pub fn find_vpd_ro(dt: &fdt::DeviceTree) -> Option<Vec<u8>> {
    let table = find_table(dt)?;
    unsafe {
        // struct lb_cbmem_ref
        let entry = find_entry(table, LB_TAG_VPD, true)?;
        if read_u32(entry + 4) < 16 {
            return None;
        }
        let vpd = read_u64(entry + 8) as usize;
        if vpd == 0 || (vpd & 3) != 0 {
            return None;
        }
        // CBMEM may have been left in the data cache by the previous boot stage
        arch::cache::dcache_clean_invalidate(vpd, VPD_CBMEM_HEADER_SIZE);
        if read_u32(vpd) != VPD_CBMEM_MAGIC || read_u32(vpd + 4) != VPD_CBMEM_VERSION {
            return None;
        }
        let ro_size = read_u32(vpd + 8) as usize;
        if ro_size > MAX_VPD_SIZE {
            return None;
        }
        let ro = vpd + VPD_CBMEM_HEADER_SIZE;
        arch::cache::dcache_clean_invalidate(ro, ro_size);
        Some((0..ro_size).map(|i| read_u8(ro + i)).collect())
    }
}

/// Returns the address of the coreboot table from `/firmware/coreboot`.
fn find_table(dt: &fdt::DeviceTree) -> Option<usize> {
    let root = dt.root();
    let firmware = root.find_child_exact(NodeName::new("firmware"))?;
    // Old depthcharge adds /firmware without #address-cells, so fall back to the root's.
    let address_cells = firmware
        .address_cells()
        .map(|v| v.0)
        .unwrap_or(root.address_cells().0) as usize;

    for node in firmware.children() {
        if node.is_compatible_with("coreboot") {
            let reg = node.get_prop(PropName::REG)?;
            let words = reg.words();
            if address_cells == 0 || words.len() < address_cells {
                return None;
            }
            let address = words[..address_cells]
                .iter()
                .fold(0u64, |acc, v| (acc << 32) | v.as_u32() as u64);
            return Some(address as usize);
        }
    }
    None
}

/// Finds the first entry with `tag`, following a forward entry once.
unsafe fn find_entry(table: usize, tag: u32, follow_forward: bool) -> Option<usize> {
    unsafe {
        if table == 0 || (table & 3) != 0 {
            return None;
        }
        // The table may have been left in the data cache by the previous boot stage
        crate::arch::cache::dcache_clean_invalidate(table, HEADER_SIZE);
        if read_u32(table) != SIGNATURE {
            return None;
        }
        let header_bytes = read_u32(table + 4) as usize;
        let table_bytes = read_u32(table + 12) as usize;
        let table_entries = read_u32(table + 20);
        if table_bytes > MAX_TABLE_SIZE {
            return None;
        }
        crate::arch::cache::dcache_clean_invalidate(table + header_bytes, table_bytes);

        let mut entry = table + header_bytes;
        let end = entry + table_bytes;
        for _ in 0..table_entries {
            if entry + 8 > end {
                break;
            }
            let entry_tag = read_u32(entry);
            let entry_size = read_u32(entry + 4) as usize;
            if entry_size < 8 || entry + entry_size > end {
                break;
            }
            if entry_tag == tag {
                return Some(entry);
            }
            if entry_tag == LB_TAG_FORWARD && follow_forward && entry_size >= 16 {
                return find_entry(read_u64(entry + 8) as usize, tag, false);
            }
            entry = (entry + entry_size + 3) & !3;
        }
        None
    }
}

/// Parses `struct lb_framebuffer`.
unsafe fn parse_framebuffer(entry: usize) -> Option<Framebuffer> {
    unsafe {
        if read_u32(entry + 4) < 35 {
            return None;
        }
        Some(Framebuffer {
            base: read_u64(entry + 8) as usize,
            width: read_u32(entry + 16) as usize,
            height: read_u32(entry + 20) as usize,
            stride: read_u32(entry + 24) as usize,
            bits_per_pixel: read_u8(entry + 28),
            red: (read_u8(entry + 29), read_u8(entry + 30)),
            green: (read_u8(entry + 31), read_u8(entry + 32)),
            blue: (read_u8(entry + 33), read_u8(entry + 34)),
        })
    }
}

#[inline]
unsafe fn read_u8(address: usize) -> u8 {
    unsafe { (address as *const u8).read_volatile() }
}

#[inline]
unsafe fn read_u32(address: usize) -> u32 {
    unsafe { (address as *const u32).read_volatile() }
}

#[inline]
unsafe fn read_u64(address: usize) -> u64 {
    unsafe { read_u32(address) as u64 | ((read_u32(address + 4) as u64) << 32) }
}

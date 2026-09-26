//! Choose the early allocator's one contiguous, 32-bit addressable RAM span.
//! The allocator still has a u32 base/size, so never silently truncate a DT
//! address. Firmware, the DT blob and fixed `/reserved-memory` children are
//! excluded before the allocator's metadata is placed at the end of the span.

use fdt::{DeviceTree, PropName};

const PAGE: u64 = 4096;
const MAX_EARLY_RAM: u64 = 64 * 1024 * 1024;

fn align_up(value: u64) -> Option<u64> {
    value.checked_add(PAGE - 1).map(|v| v & !(PAGE - 1))
}

fn chosen_address(tree: &DeviceTree, name: &str) -> Option<u64> {
    let chosen = tree.root().chosen()?;
    let prop = chosen.get_prop(PropName(name))?;
    let words = prop.words();
    match words {
        [lo] => Some(lo.as_u32() as u64),
        [hi, lo] => Some(((hi.as_u32() as u64) << 32) | lo.as_u32() as u64),
        _ => None,
    }
}

fn reservations(tree: &DeviceTree, mut visit: impl FnMut(u64, u64)) {
    let (dtb, len) = tree.range();
    visit(dtb as u64, len as u64);
    for (base, len) in tree.header().reserved_maps() {
        visit(base, len);
    }
    if let Some(parent) = tree.root().reserved_memory() {
        for node in parent.children() {
            if let Some(regions) = node.reg() {
                for (base, len) in regions {
                    visit(base, len);
                }
            }
        }
    }
    if let (Some(start), Some(end)) = (
        chosen_address(tree, "linux,initrd-start"),
        chosen_address(tree, "linux,initrd-end"),
    ) {
        if end > start {
            visit(start, end - start);
        }
    }
}

/// Returns a page-aligned region after the kernel, within the same DT memory
/// bank and below 4 GiB. Never overlaps a fixed reservation. Dynamic CMA
/// declarations without `reg` have not been allocated by firmware here.
pub(super) fn early_ram(tree: &DeviceTree, kernel_end: u64) -> Option<(u32, u32)> {
    let mut start = align_up(kernel_end)?;
    let (bank, size) = tree.memory_map()?.find(|&(base, size)| {
        base <= start && base.checked_add(size).is_some_and(|end| start < end)
    })?;
    let bank_end = bank.checked_add(size)?.min(0x1_0000_0000);
    if bank_end <= start {
        return None;
    }

    // A DTB or an initrd can be placed immediately after the image. Advance
    // past such a region, then scan again because reservations may overlap.
    for _ in 0..32 {
        let mut next = start;
        reservations(tree, |base, len| {
            if let Some(end) = base.checked_add(len)
                && base <= start
                && start < end
            {
                next = next.max(end);
            }
        });
        if next == start {
            break;
        }
        start = align_up(next)?;
    }
    let mut still_reserved = false;
    reservations(tree, |base, len| {
        if let Some(end) = base.checked_add(len)
            && base <= start
            && start < end
        {
            still_reserved = true;
        }
    });
    if still_reserved {
        return None;
    }
    let mut end = bank_end.min(start.checked_add(MAX_EARLY_RAM)?);
    reservations(tree, |base, len| {
        if let Some(reserved_end) = base.checked_add(len)
            && base > start
            && reserved_end > start
        {
            end = end.min(base);
        }
    });
    end &= !(PAGE - 1);
    // The allocator stores its own 64 KiB map at the end of this span.
    if end.checked_sub(start)? < 128 * 1024 {
        return None;
    }
    Some((u32::try_from(start).ok()?, u32::try_from(end - start).ok()?))
}

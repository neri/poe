//! Collecting the memory map inputs from a device tree
//!
//! The device tree is only read; nothing is written back to it. What MiniOS
//! learns here goes to its own map, and the DTB is handed over as it came.

use fdt::{DeviceTree, Node, PropName};

use super::region::MapBuilder;
use super::{MapError, MemoryType, PhysRange, RegionAttrs};

/// What was found besides the ranges added to the builder
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DtSummary {
    /// `/reserved-memory` children with only a `size`. MiniOS does not
    /// allocate them; the OS gets the unchanged DTB and handles them.
    pub dynamic_reservations: usize,
}

/// Adds the RAM and the protected ranges described by the device tree:
///
/// - `reg` of every enabled memory node (RAM)
/// - the memory reservation block (`Reserved`, `MEMRESERVE`)
/// - fixed `reg` of enabled `/reserved-memory` children (`Reserved`,
///   `RESERVED_MEMORY`, with `NO_MAP` or `REUSABLE`)
/// - the DTB itself (`DeviceTree`)
/// - an initrd from `/chosen` (`Initrd`)
///
/// Anything that cannot be read is an error: a reservation that is not
/// understood must not become free memory.
pub fn collect<const R: usize, const P: usize>(
    dt: &DeviceTree,
    builder: &mut MapBuilder<R, P>,
) -> Result<DtSummary, MapError> {
    let mut summary = DtSummary::default();
    let root = dt.root();
    let address_cells = root.address_cells();
    let size_cells = root.size_cells();

    for node in dt.memory_nodes() {
        let regs = node
            .reg_with_cells(address_cells, size_cells)
            .map_err(|_| MapError::Malformed("memory node reg"))?;
        for (base, size) in regs.into_iter().flatten() {
            if let Some(range) = PhysRange::from_base_size(base, size)? {
                builder.add_ram(range)?;
            }
        }
    }

    for entry in dt.header().reserved_map_entries() {
        let (base, size) = entry.map_err(|_| MapError::Malformed("memory reservation block"))?;
        if let Some(range) = PhysRange::from_base_size(base, size)? {
            builder.add_protected(range, MemoryType::Reserved, RegionAttrs::MEMRESERVE)?;
        }
    }

    if let Some(parent) = root.reserved_memory() {
        let address_cells = parent.address_cells().unwrap_or(address_cells);
        let size_cells = parent.size_cells().unwrap_or(size_cells);
        for node in parent.children().filter(Node::status_is_ok) {
            let no_map = node.get_prop(PropName::NO_MAP).is_some();
            let reusable = node.get_prop(PropName::REUSABLE).is_some();
            if no_map && reusable {
                return Err(MapError::Malformed(
                    "reserved-memory with no-map and reusable",
                ));
            }
            let mut attrs = RegionAttrs::RESERVED_MEMORY;
            attrs.set(RegionAttrs::NO_MAP, no_map);
            attrs.set(RegionAttrs::REUSABLE, reusable);
            let regs = node
                .reg_with_cells(address_cells, size_cells)
                .map_err(|_| MapError::Malformed("reserved-memory reg"))?;
            match regs {
                Some(regs) => {
                    for (base, size) in regs {
                        if let Some(range) = PhysRange::from_base_size(base, size)? {
                            builder.add_protected(range, MemoryType::Reserved, attrs)?;
                        }
                    }
                }
                None => {
                    if node.get_prop(PropName::new("size")).is_some() {
                        summary.dynamic_reservations += 1;
                    }
                }
            }
        }
    }

    let (dtb, dtb_size) = dt.range();
    if let Some(range) = PhysRange::from_base_size(dtb as usize as u64, dtb_size as u64)? {
        builder.add_protected(range, MemoryType::DeviceTree, RegionAttrs::empty())?;
    }

    if let Some(chosen) = root.chosen() {
        let start = chosen_address(&chosen, "linux,initrd-start")?;
        let end = chosen_address(&chosen, "linux,initrd-end")?;
        match (start, end) {
            (Some(start), Some(end)) => {
                if let Some(range) = PhysRange::from_bounds(start, end)? {
                    builder.add_protected(range, MemoryType::Initrd, RegionAttrs::empty())?;
                }
            }
            (None, None) => {}
            _ => return Err(MapError::Malformed("chosen initrd range")),
        }
    }

    Ok(summary)
}

/// Protects the RAM from the start of the bank that contains `address` up to
/// `address`, as `mem_type` with `attrs`.
///
/// For firmware that sits below the payload without the device tree saying
/// so. Call it after [`collect`], which adds the banks. Nothing is added if no
/// bank contains `address` or the bank starts there.
pub fn protect_ram_below<const R: usize, const P: usize>(
    builder: &mut MapBuilder<R, P>,
    address: u64,
    mem_type: MemoryType,
    attrs: RegionAttrs,
) -> Result<(), MapError> {
    let start = builder
        .ram()
        .iter()
        .filter(|v| v.contains(address))
        .map(|v| v.start())
        .min();
    match start.and_then(|start| PhysRange::new(start, address)) {
        Some(range) => builder.add_protected(range, mem_type, attrs),
        None => Ok(()),
    }
}

fn chosen_address(chosen: &Node, name: &str) -> Result<Option<u64>, MapError> {
    let Some(prop) = chosen.get_prop(PropName::new(name)) else {
        return Ok(None);
    };
    match (prop.len(), prop.words()) {
        (4, [value]) => Ok(Some(value.as_u32() as u64)),
        (8, [hi, lo]) => Ok(Some(((hi.as_u32() as u64) << 32) | lo.as_u32() as u64)),
        _ => Err(MapError::Malformed("chosen initrd address")),
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use fdt::builder::{FdtBuilder, as_bytes};

    use super::*;
    use crate::mem::MemoryRegion;

    type Builder = MapBuilder<16, 32>;

    fn tree(build: impl FnOnce(&mut FdtBuilder)) -> Vec<u64> {
        let mut b = FdtBuilder::new();
        b.begin_node("")
            .prop_u32("#address-cells", 2)
            .prop_u32("#size-cells", 2);
        build(&mut b);
        b.end_node();
        b.build()
    }

    fn collect_from(blob: &[u64]) -> Result<(Builder, DtSummary), MapError> {
        let dt = DeviceTree::from_slice(as_bytes(blob)).unwrap();
        let mut builder = Builder::new();
        collect(&dt, &mut builder).map(|summary| (builder, summary))
    }

    fn protected(builder: &Builder) -> Vec<(u64, u64, MemoryType, RegionAttrs)> {
        builder
            .protected()
            .iter()
            .filter(|v| v.mem_type != MemoryType::DeviceTree)
            .map(|v| (v.range.start(), v.range.end(), v.mem_type, v.attrs))
            .collect()
    }

    fn full_tree(b: &mut FdtBuilder) {
        b.reserve(0x4000_0000, 0x1000);
        b.begin_node("chosen")
            .prop_cells("linux,initrd-start", &[0x4800_0000])
            .prop_cells("linux,initrd-end", &[0, 0x4810_0000])
            .end_node();
        b.begin_node("memory@40000000")
            .prop_str("device_type", "memory")
            .prop_cells("reg", &[0, 0x4000_0000, 0, 0x2000_0000])
            .end_node();
        b.begin_node("memory@100000000")
            .prop_str("device_type", "memory")
            .prop_cells("reg", &[1, 0, 1, 0])
            .end_node();
        b.begin_node("reserved-memory")
            .prop_u32("#address-cells", 2)
            .prop_u32("#size-cells", 2)
            .prop_empty("ranges");
        b.begin_node("firmware@40100000")
            .prop_cells("reg", &[0, 0x4010_0000, 0, 0x8_0000])
            .prop_empty("no-map")
            .end_node();
        b.begin_node("pool@50000000")
            .prop_cells("reg", &[0, 0x5000_0000, 0, 0x40_0000])
            .prop_empty("reusable")
            .end_node();
        b.begin_node("linux,cma")
            .prop_cells("size", &[0, 0x400_0000])
            .prop_empty("reusable")
            .end_node();
        b.begin_node("off@58000000")
            .prop_str("status", "disabled")
            .prop_cells("reg", &[0, 0x5800_0000, 0, 0x1000])
            .end_node();
        b.end_node();
    }

    #[test]
    fn everything_the_device_tree_describes_is_collected() {
        let blob = tree(full_tree);
        let (builder, summary) = collect_from(&blob).unwrap();
        let ram: Vec<_> = builder.ram().iter().map(|v| (v.start(), v.end())).collect();
        assert_eq!(
            ram,
            [(0x4000_0000, 0x6000_0000), (0x1_0000_0000, 0x2_0000_0000)]
        );
        use MemoryType::*;
        assert_eq!(
            protected(&builder),
            [
                (0x4000_0000, 0x4000_1000, Reserved, RegionAttrs::MEMRESERVE),
                (
                    0x4010_0000,
                    0x4018_0000,
                    Reserved,
                    RegionAttrs::RESERVED_MEMORY | RegionAttrs::NO_MAP
                ),
                (
                    0x5000_0000,
                    0x5040_0000,
                    Reserved,
                    RegionAttrs::RESERVED_MEMORY | RegionAttrs::REUSABLE
                ),
                (0x4800_0000, 0x4810_0000, Initrd, RegionAttrs::empty()),
            ]
        );
        assert_eq!(summary.dynamic_reservations, 1);
        let dtb = builder
            .protected()
            .iter()
            .find(|v| v.mem_type == DeviceTree)
            .unwrap();
        assert_eq!(dtb.range.start(), blob.as_ptr() as u64);
        let total_size = u32::from_be_bytes(as_bytes(&blob)[4..8].try_into().unwrap());
        assert_eq!(dtb.range.len(), total_size as u64);
    }

    #[test]
    fn the_device_tree_blob_is_not_changed() {
        let blob = tree(full_tree);
        let before = blob.clone();
        let (builder, _) = collect_from(&blob).unwrap();
        let mut out = vec![
            MemoryRegion::new(
                PhysRange::new(0, 1).unwrap(),
                MemoryType::Available,
                RegionAttrs::empty()
            );
            128
        ];
        builder.build(&mut out).unwrap();
        assert_eq!(blob, before);
    }

    #[test]
    fn unreadable_reservations_stop_the_collection() {
        let cases: [fn(&mut FdtBuilder); 5] = [
            |b| {
                b.begin_node("memory@0")
                    .prop_cells("reg", &[0, 0x4000_0000, 0])
                    .end_node();
            },
            |b| {
                b.begin_node("reserved-memory")
                    .prop_u32("#address-cells", 3)
                    .prop_u32("#size-cells", 2);
                b.begin_node("x@0")
                    .prop_cells("reg", &[0, 0, 0x4000_0000, 0, 0x1000])
                    .end_node();
                b.end_node();
            },
            |b| {
                b.begin_node("reserved-memory")
                    .prop_u32("#address-cells", 2)
                    .prop_u32("#size-cells", 2);
                b.begin_node("x@0")
                    .prop_cells("reg", &[0, 0x4000_0000, 0, 0x1000])
                    .prop_empty("no-map")
                    .prop_empty("reusable")
                    .end_node();
                b.end_node();
            },
            |b| {
                b.begin_node("chosen")
                    .prop_cells("linux,initrd-start", &[0x4800_0000])
                    .end_node();
            },
            |b| {
                b.begin_node("chosen")
                    .prop_cells("linux,initrd-start", &[0x4800_0000])
                    .prop_cells("linux,initrd-end", &[0x4700_0000])
                    .end_node();
            },
        ];
        for build in cases {
            let blob = tree(build);
            assert!(collect_from(&blob).is_err());
        }
        // A reservation whose end does not fit in 64 bits
        let blob = tree(|b| {
            b.reserve(u64::MAX - 0xfff, 0x2000);
        });
        assert_eq!(collect_from(&blob).err(), Some(MapError::Overflow));
    }

    #[test]
    fn empty_ranges_are_skipped() {
        let blob = tree(|b| {
            b.reserve(0x4000_0000, 0);
            b.begin_node("memory@0")
                .prop_cells("reg", &[0, 0, 0, 0, 0, 0x4000_0000, 0, 0x1000_0000])
                .end_node();
            b.begin_node("chosen")
                .prop_cells("linux,initrd-start", &[0x4800_0000])
                .prop_cells("linux,initrd-end", &[0x4800_0000])
                .end_node();
        });
        let (builder, _) = collect_from(&blob).unwrap();
        assert_eq!(builder.ram().len(), 1);
        assert!(protected(&builder).is_empty());
    }

    #[test]
    fn ram_below_the_image_can_be_protected() {
        let mut builder = Builder::new();
        builder
            .add_ram(PhysRange::new(0x4000_0000, 0x8000_0000).unwrap())
            .unwrap();
        builder
            .add_ram(PhysRange::new(0x1_0000_0000, 0x2_0000_0000).unwrap())
            .unwrap();
        let fw = RegionAttrs::FIRMWARE;
        protect_ram_below(&mut builder, 0x4020_0000, MemoryType::OtherFw, fw).unwrap();
        // At the start of a bank, or outside every bank: nothing
        protect_ram_below(&mut builder, 0x1_0000_0000, MemoryType::OtherFw, fw).unwrap();
        protect_ram_below(&mut builder, 0x9000_0000, MemoryType::OtherFw, fw).unwrap();
        assert_eq!(
            protected(&builder),
            [(0x4000_0000, 0x4020_0000, MemoryType::OtherFw, fw)]
        );
        // The image itself is still allowed right above it
        builder
            .add_protected(
                PhysRange::new(0x4020_0000, 0x4025_0000).unwrap(),
                MemoryType::Loader,
                RegionAttrs::empty(),
            )
            .unwrap();
        let mut out = vec![
            MemoryRegion::new(
                PhysRange::new(0, 1).unwrap(),
                MemoryType::Available,
                RegionAttrs::empty()
            );
            16
        ];
        let len = builder.build(&mut out).unwrap();
        assert_eq!(
            out[0].range,
            PhysRange::new(0x4000_0000, 0x4020_0000).unwrap()
        );
        assert_eq!(out[0].mem_type, MemoryType::OtherFw);
        assert!(
            out[..len]
                .iter()
                .all(|v| !v.is_free() || v.range.start() >= 0x4025_0000)
        );
    }

    #[test]
    fn qemu_virt() {
        let bytes = include_bytes!("../../../lib/fdt/tests/data/qemu-virt-gicv3.dtb");
        let mut blob = vec![0u64; bytes.len().div_ceil(8)];
        unsafe {
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                blob.as_mut_ptr() as *mut u8,
                bytes.len(),
            );
        }
        let (builder, summary) = collect_from(&blob).unwrap();
        let ram: Vec<_> = builder.ram().iter().map(|v| (v.start(), v.end())).collect();
        assert_eq!(ram, [(0x4000_0000, 0x6000_0000)]);
        assert!(protected(&builder).is_empty());
        assert_eq!(summary.dynamic_reservations, 0);
    }

    #[test]
    fn raspberry_pi_4_before_the_firmware_fills_it_in() {
        // The firmware writes the memory size at boot; the file has zeros
        let bytes = include_bytes!("../../../lib/fdt/tests/data/bcm2711-rpi-4-b.dtb");
        let mut blob = vec![0u64; bytes.len().div_ceil(8)];
        unsafe {
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                blob.as_mut_ptr() as *mut u8,
                bytes.len(),
            );
        }
        let (builder, summary) = collect_from(&blob).unwrap();
        assert!(builder.ram().is_empty());
        assert_eq!(
            protected(&builder),
            [(0, 0x1000, MemoryType::Reserved, RegionAttrs::MEMRESERVE)]
        );
        assert_eq!(summary.dynamic_reservations, 1);
    }
}

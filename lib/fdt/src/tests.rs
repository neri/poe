use alloc::vec::Vec;

use crate::builder::{FdtBuilder, as_bytes};
use crate::*;

fn tree_with(build: impl FnOnce(&mut FdtBuilder)) -> Vec<u64> {
    let mut b = FdtBuilder::new();
    b.begin_node("")
        .prop_u32("#address-cells", 2)
        .prop_u32("#size-cells", 2);
    build(&mut b);
    b.end_node();
    b.build()
}

#[test]
fn every_enabled_memory_node_and_every_reg_entry_is_listed() {
    let blob = tree_with(|b| {
        b.begin_node("memory@40000000")
            .prop_str("device_type", "memory")
            .prop_cells(
                "reg",
                &[0, 0x4000_0000, 0, 0x1000_0000, 1, 0, 0, 0x2000_0000],
            )
            .end_node();
        b.begin_node("memory@80000000")
            .prop_str("device_type", "memory")
            .prop_cells("reg", &[0, 0x8000_0000, 0, 0x100_0000])
            .end_node();
        b.begin_node("memory@90000000")
            .prop_str("status", "disabled")
            .prop_cells("reg", &[0, 0x9000_0000, 0, 0x100_0000])
            .end_node();
        b.begin_node("ram")
            .prop_str("device_type", "memory")
            .prop_cells("reg", &[0, 0xa000_0000, 0, 0x1000])
            .end_node();
    });
    let dt = DeviceTree::from_slice(as_bytes(&blob)).unwrap();
    let map: Vec<_> = dt.memory_map().unwrap().collect();
    assert_eq!(
        map,
        [
            (0x4000_0000, 0x1000_0000),
            (0x1_0000_0000, 0x2000_0000),
            (0x8000_0000, 0x100_0000),
            (0xa000_0000, 0x1000),
        ]
    );
    assert_eq!(dt.memory_nodes().count(), 3);
}

#[test]
fn malformed_reg_is_an_error_not_a_short_list() {
    let blob = tree_with(|b| {
        b.begin_node("memory@0")
            .prop_cells("reg", &[0, 0x4000_0000, 0, 0x1000_0000, 0])
            .end_node();
    });
    let dt = DeviceTree::from_slice(as_bytes(&blob)).unwrap();
    let node = dt.memory_nodes().next().unwrap();
    let root = dt.root();
    assert_eq!(
        node.reg_with_cells(root.address_cells(), root.size_cells())
            .err(),
        Some(RegError::Truncated)
    );
    assert_eq!(
        node.reg_with_cells(AddressCells(3), SizeCells(2)).err(),
        Some(RegError::UnsupportedCells)
    );
    // The lenient iterator skips the whole malformed property
    assert_eq!(dt.memory_map().unwrap().count(), 0);
}

#[test]
fn reservation_block_ends_only_at_the_zero_terminator() {
    let mut b = FdtBuilder::new();
    b.begin_node("").end_node();
    b.raw_reservations(&[
        (0x1000, 0x2000),
        (0x5000, 0),
        (0x8000, 0x1000),
        (0, 0),
        (0x9000, 1),
    ]);
    let blob = b.build();
    let dt = DeviceTree::from_slice(as_bytes(&blob)).unwrap();
    let entries: Vec<_> = dt.header().reserved_map_entries().collect();
    assert_eq!(
        entries,
        [Ok((0x1000, 0x2000)), Ok((0x5000, 0)), Ok((0x8000, 0x1000))]
    );
    let nonempty: Vec<_> = dt.header().reserved_maps().collect();
    assert_eq!(nonempty, [(0x1000, 0x2000), (0x8000, 0x1000)]);
}

#[test]
fn reservation_block_without_terminator_is_reported() {
    let mut b = FdtBuilder::new();
    b.begin_node("").end_node();
    b.raw_reservations(&[(0x1000, 0x2000)]);
    let mut blob = b.build();
    // Make the structure block start right after the only entry and shrink
    // the blob so that no terminator fits.
    let bytes =
        unsafe { core::slice::from_raw_parts_mut(blob.as_mut_ptr() as *mut u8, blob.len() * 8) };
    let rsvmap = u32::from_be_bytes(bytes[16..20].try_into().unwrap()) as usize;
    bytes[4..8].copy_from_slice(&((rsvmap + 16) as u32).to_be_bytes());
    let header = unsafe { &*(bytes.as_ptr() as *const Header) };
    let entries: Vec<_> = header.reserved_map_entries().collect();
    assert_eq!(
        entries,
        [Ok((0x1000, 0x2000)), Err(ParseError::InvalidData)]
    );
}

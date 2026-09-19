//! Tests of `fdt::bus` with the device trees in `data/` (see `data/README.md`)

use fdt::DeviceTree;
use fdt::bus::{self, AddressMap};

/// Parses a DTB, copied to an 8-byte aligned buffer.
fn parse(blob: &[u8], f: impl FnOnce(&DeviceTree)) {
    let mut buf = vec![0u64; blob.len().div_ceil(8)];
    unsafe {
        core::ptr::copy_nonoverlapping(blob.as_ptr(), buf.as_mut_ptr() as *mut u8, blob.len());
    }
    let dt = unsafe { DeviceTree::parse(buf.as_ptr() as *const u8) }.unwrap();
    f(&dt)
}

fn base(dt: &DeviceTree, compatible: &[&str], index: usize) -> Option<u64> {
    bus::find_reg(dt, compatible, index).map(|(base, _size)| base)
}

#[test]
fn root_nodes() {
    parse(include_bytes!("data/bus.dtb"), |dt| {
        assert_eq!(
            bus::find_reg(dt, &["test,root-dev"], 0),
            Some((0x1000_0000, 0x100))
        );
        assert_eq!(
            bus::find_reg(dt, &["test,root-dev"], 1),
            Some((0x1000_1000, 0x200))
        );
        assert_eq!(bus::find_reg(dt, &["test,root-dev"], 2), None);
        assert_eq!(base(dt, &["test,no-such-dev"], 0), None);
    });
}

#[test]
fn compatible_with_any() {
    parse(include_bytes!("data/bus.dtb"), |dt| {
        assert_eq!(
            base(dt, &["test,no-such-dev", "test,root-dev"], 0),
            Some(0x1000_0000)
        );
        assert!(dt.root().is_compatible_with_any(&["x", "test,bus"]));
        assert!(!dt.root().is_compatible_with_any(&["x", "test"]));
        assert!(!dt.root().is_compatible_with_any(&[]));
    });
}

#[test]
fn disabled_nodes_are_skipped() {
    parse(include_bytes!("data/bus.dtb"), |dt| {
        assert_eq!(base(dt, &["test,disabled-dev"], 0), Some(0x1200_0000));
        assert_eq!(base(dt, &["test,hidden-dev"], 0), None);
    });
}

#[test]
fn translation() {
    parse(include_bytes!("data/bus.dtb"), |dt| {
        // `ranges;`
        assert_eq!(base(dt, &["test,identity-dev"], 0), Some(0x1300_0000));
        // the first and the second range, to a 64-bit address
        assert_eq!(
            bus::find_reg(dt, &["test,uart"], 0),
            Some((0x1_3f20_1000, 0x200))
        );
        assert_eq!(base(dt, &["test,intc"], 0), Some(0x4000_0000));
        // nested buses
        assert_eq!(base(dt, &["test,nested-dev"], 0), Some(0x1_3f80_1000));
        assert_eq!(base(dt, &["test,level3-dev"], 0), Some(0x1_3f80_2000));
    });
}

#[test]
fn not_searched() {
    parse(include_bytes!("data/bus.dtb"), |dt| {
        // below a node without `ranges`
        assert_eq!(base(dt, &["test,spi-child"], 0), None);
        // deeper than MAX_DEPTH
        assert_eq!(AddressMap::MAX_DEPTH, 3);
        assert_eq!(base(dt, &["test,level4-dev"], 0), None);
        // below a bus whose `ranges` cannot be parsed
        assert_eq!(base(dt, &["test,no-cells-dev"], 0), None);
    });
}

#[test]
fn unmapped_address_continues_the_search() {
    parse(include_bytes!("data/bus.dtb"), |dt| {
        assert_eq!(base(dt, &["test,unmapped"], 0), Some(0x1600_0000));
    });
}

#[test]
fn enter_and_translate() {
    parse(include_bytes!("data/bus.dtb"), |dt| {
        let soc = dt
            .root()
            .children()
            .find(|v| v.name().as_str() == "soc")
            .unwrap();
        let map = AddressMap::new().enter(&soc).unwrap();
        assert_eq!(map.translate(0x7e00_0000), Some(0x1_3f00_0000));
        assert_eq!(map.translate(0x7eff_ffff), Some(0x1_3fff_ffff));
        assert_eq!(map.translate(0x7f00_0000), None);
        assert_eq!(map.translate(0x4000_1000), None);
        // the root is the identity map
        assert_eq!(AddressMap::new().translate(0x7e00_0000), Some(0x7e00_0000));
        // not a bus
        let dev = dt
            .root()
            .children()
            .find(|v| v.name().as_str() == "root-dev@10000000");
        assert!(AddressMap::new().enter(&dev.unwrap()).is_none());
    });
}

#[test]
fn raspberry_pi_3() {
    parse(include_bytes!("data/bcm2710-rpi-3-b.dtb"), |dt| {
        assert_eq!(base(dt, &["arm,pl011"], 0), Some(0x3f20_1000));
        assert_eq!(base(dt, &["brcm,bcm2836-l1-intc"], 0), Some(0x4000_0000));
        assert_eq!(base(dt, &["brcm,bcm2835-pm-wdt"], 0), Some(0x3f10_0000));
        assert_eq!(
            base(dt, &["arm,gic-400", "arm,cortex-a15-gic", "arm,gic-v3"], 0),
            None
        );
        assert_eq!(base(dt, &["arm,psci-1.0", "arm,psci-0.2"], 0), None);
    });
}

#[test]
fn raspberry_pi_4() {
    parse(include_bytes!("data/bcm2711-rpi-4-b.dtb"), |dt| {
        assert_eq!(base(dt, &["arm,pl011"], 0), Some(0xfe20_1000));
        assert_eq!(base(dt, &["arm,gic-400"], 0), Some(0xff84_1000));
        assert_eq!(base(dt, &["arm,gic-400"], 1), Some(0xff84_2000));
        assert_eq!(base(dt, &["brcm,bcm2835-pm-wdt"], 0), Some(0xfe10_0000));
    });
}

#[test]
fn bob() {
    parse(include_bytes!("data/bob.dtb"), |dt| {
        assert_eq!(base(dt, &["arm,gic-v3"], 0), Some(0xfee0_0000));
        assert_eq!(base(dt, &["rockchip,rk3399-vop-big"], 0), Some(0xff90_0000));
        assert_eq!(base(dt, &["rockchip,rk3066-spi"], 0), Some(0xff20_0000));
        // below the SPI controller
        assert_eq!(base(dt, &["google,cros-ec-spi"], 0), None);
        assert_eq!(base(dt, &["arm,pl011"], 0), None);
    });
}

#[test]
fn qemu_virt() {
    parse(include_bytes!("data/qemu-virt-gicv3.dtb"), |dt| {
        assert_eq!(base(dt, &["arm,pl011"], 0), Some(0x0900_0000));
        assert_eq!(base(dt, &["arm,gic-v3"], 0), Some(0x0800_0000));
        assert_eq!(base(dt, &["arm,gic-v3"], 1), Some(0x080a_0000));
    });
}

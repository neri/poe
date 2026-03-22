//! QEMU virt machine

use core::panic;
use fdt::DeviceTree;

pub(super) unsafe fn init_early(dt: &DeviceTree) {
    if !dt.root().is_compatible_with("riscv-virtio") {
        panic!("Incompatible platform");
    }
}

pub(super) unsafe fn init_late() {
    // TODO:
}

pub(super) unsafe fn exit() {
    // TODO:
}

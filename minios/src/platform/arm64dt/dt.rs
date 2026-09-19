//! Finding devices in the device tree
//!
//! Wrappers of [`fdt::bus`] that return the CPU addresses as `usize`.
//! No heap is used, so these can be used before the memory manager is initialized.

/// [`fdt::bus::AddressMap`] returning the CPU addresses as `usize`
pub struct AddressMap<'m, 'a>(&'m fdt::bus::AddressMap<'a>);

impl AddressMap<'_, '_> {
    /// Translates a bus address of the node to the CPU address.
    pub fn translate(&self, address: u64) -> Option<usize> {
        usize::try_from(self.0.translate(address)?).ok()
    }

    /// Returns the `index`-th `reg` (CPU address, size) of the node.
    pub fn reg(&self, node: &fdt::Node, index: usize) -> Option<(usize, usize)> {
        let (address, size) = self.0.reg(node, index)?;
        Some((usize::try_from(address).ok()?, size as usize))
    }
}

/// Calls `f` for each enabled node below the root (and below the buses) until it returns `Some`.
pub fn find_map<'a, T>(
    dt: &'a fdt::DeviceTree,
    mut f: impl FnMut(&fdt::Node<'a>, &AddressMap) -> Option<T>,
) -> Option<T> {
    fdt::bus::find_map(dt, |node, map| f(node, &AddressMap(map)))
}

/// Returns whether `node` is compatible with any of `compatible`.
#[inline]
pub fn is_compatible(node: &fdt::Node, compatible: &[&str]) -> bool {
    node.is_compatible_with_any(compatible)
}

/// Returns the `index`-th `reg` (CPU address, size) of the first enabled node compatible with `compatible`.
pub fn find_reg(dt: &fdt::DeviceTree, compatible: &[&str], index: usize) -> Option<(usize, usize)> {
    find_map(dt, |node, map| {
        is_compatible(node, compatible)
            .then(|| map.reg(node, index))
            .flatten()
    })
}

/// Translates `address` on the first enabled bus compatible with `compatible` to the CPU address.
///
/// Buses that do not map the address are skipped.
pub fn translate_bus_address(
    dt: &fdt::DeviceTree,
    compatible: &[&str],
    address: u64,
) -> Option<usize> {
    let address = fdt::bus::find_map(dt, |node, map| {
        is_compatible(node, compatible)
            .then(|| map.enter(node)?.translate(address))
            .flatten()
    })?;
    usize::try_from(address).ok()
}

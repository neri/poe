//! Finding devices in the device tree
//!
//! Devices may be placed under buses (e.g. `/soc` of Raspberry Pi), whose `ranges` map
//! the bus addresses to the CPU addresses. Only buses with `ranges` are searched,
//! since the addresses of the other nodes (e.g. SPI, I2C) are not memory mapped.
//! No heap is used, so these can be used before the memory manager is initialized.

use fdt::PropName;

/// Maximum depth of the buses searched below the root
const MAX_DEPTH: usize = 3;
/// Maximum number of `ranges` entries of a bus
const MAX_RANGES: usize = 8;

/// `ranges` of a bus: (child address, parent address, length), or `None` for the identity map
type Ranges = Option<heapless::Vec<(u64, u64, u64), MAX_RANGES>>;

/// Address translation from a node to the CPU (the `ranges` of its buses)
#[derive(Clone, Default)]
pub struct AddressMap {
    /// From the outermost bus
    buses: heapless::Vec<Ranges, MAX_DEPTH>,
}

impl AddressMap {
    /// Translates a bus address of the node to the CPU address.
    pub fn translate(&self, address: u64) -> Option<usize> {
        let mut address = address;
        for ranges in self.buses.iter().rev() {
            if let Some(ranges) = ranges {
                let &(child, parent, _len) = ranges
                    .iter()
                    .find(|&&(child, _, len)| address >= child && address - child < len)?;
                address = parent + (address - child);
            }
        }
        usize::try_from(address).ok()
    }

    /// Returns the `index`-th `reg` (CPU address, size) of the node.
    pub fn reg(&self, node: &fdt::Node, index: usize) -> Option<(usize, usize)> {
        let (address, size) = node.reg()?.nth(index)?;
        Some((self.translate(address)?, size as usize))
    }

    /// Returns the map for the children of `bus`, or `None` if it is not a bus or too deep.
    fn enter(&self, bus: &fdt::Node) -> Option<Self> {
        let prop = bus.get_prop(PropName::RANGES)?;
        let ranges = if prop.words().is_empty() {
            None
        } else {
            Some(
                bus.ranges()?
                    .map(|v| (v.child, v.parent, v.len))
                    .take(MAX_RANGES)
                    .collect(),
            )
        };
        let mut result = self.clone();
        result.buses.push(ranges).ok()?;
        Some(result)
    }
}

/// Calls `f` for each enabled node below the root (and below the buses) until it returns `Some`.
pub fn find_map<'a, T>(
    dt: &'a fdt::DeviceTree,
    mut f: impl FnMut(&fdt::Node<'a>, &AddressMap) -> Option<T>,
) -> Option<T> {
    visit(dt.root().children(), &AddressMap::default(), &mut f)
}

fn visit<'a, T>(
    nodes: impl Iterator<Item = fdt::Node<'a>>,
    map: &AddressMap,
    f: &mut dyn FnMut(&fdt::Node<'a>, &AddressMap) -> Option<T>,
) -> Option<T> {
    for node in nodes {
        if !node.status_is_ok() {
            continue;
        }
        if let Some(result) = f(&node, map) {
            return Some(result);
        }
        if let Some(child_map) = map.enter(&node)
            && let Some(result) = visit(node.children(), &child_map, f)
        {
            return Some(result);
        }
    }
    None
}

/// Returns whether `node` is compatible with any of `compatible`.
pub fn is_compatible(node: &fdt::Node, compatible: &[&str]) -> bool {
    compatible.iter().any(|v| node.is_compatible_with(v))
}

/// Returns the `index`-th `reg` (CPU address, size) of the first enabled node compatible with `compatible`.
pub fn find_reg(dt: &fdt::DeviceTree, compatible: &[&str], index: usize) -> Option<(usize, usize)> {
    find_map(dt, |node, map| {
        is_compatible(node, compatible)
            .then(|| map.reg(node, index))
            .flatten()
    })
}

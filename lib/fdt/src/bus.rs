//! Finding memory mapped devices below the buses
//!
//! Devices may be placed under buses (e.g. `/soc` of Raspberry Pi), whose `ranges` map
//! the bus addresses to the CPU addresses. Only buses with `ranges` are searched,
//! since the addresses of the other nodes (e.g. SPI, I2C) are not memory mapped.
//! No heap is used, so these can be used before the memory manager is initialized.

use crate::{DeviceTree, Node, PropName};

/// Maximum depth of the buses searched below the root
const MAX_DEPTH: usize = 3;

/// Address translation from a node to the CPU (the `ranges` of its buses)
#[derive(Clone)]
pub struct AddressMap<'a> {
    /// From the outermost bus
    buses: [Option<Node<'a>>; MAX_DEPTH],
    depth: usize,
}

impl Default for AddressMap<'_> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> AddressMap<'a> {
    /// Maximum depth of the buses searched below the root
    pub const MAX_DEPTH: usize = MAX_DEPTH;

    /// The map of the children of the root (the identity map)
    #[inline]
    pub const fn new() -> Self {
        Self {
            buses: [const { None }; MAX_DEPTH],
            depth: 0,
        }
    }

    /// Translates a bus address of the node to the CPU address.
    ///
    /// Returns `None` if no `ranges` entry of a bus contains the address.
    pub fn translate(&self, address: u64) -> Option<u64> {
        let mut address = address;
        for bus in self.buses[..self.depth].iter().rev().flatten() {
            // `ranges;` is the identity map
            if bus.get_prop(PropName::RANGES)?.words().is_empty() {
                continue;
            }
            let range = bus
                .ranges()?
                .find(|v| address >= v.child && address - v.child < v.len)?;
            address = range.parent + (address - range.child);
        }
        Some(address)
    }

    /// Returns the `index`-th `reg` (CPU address, size) of the node.
    pub fn reg(&self, node: &Node, index: usize) -> Option<(u64, u64)> {
        let (address, size) = node.reg()?.nth(index)?;
        Some((self.translate(address)?, size))
    }

    /// Returns the map for the children of `bus`, or `None` if it is not a bus or too deep.
    pub fn enter(&self, bus: &Node<'a>) -> Option<Self> {
        let prop = bus.get_prop(PropName::RANGES)?;
        // The bus must have `#address-cells` and `#size-cells` to parse `ranges`
        if !prop.words().is_empty() && bus.ranges().is_none() {
            return None;
        }
        if self.depth >= Self::MAX_DEPTH {
            return None;
        }
        let mut result = self.clone();
        result.buses[result.depth] = Some(bus.clone());
        result.depth += 1;
        Some(result)
    }
}

/// Calls `f` for each enabled node below the root (and below the buses) until it returns `Some`.
pub fn find_map<'a, T>(
    dt: &'a DeviceTree,
    mut f: impl FnMut(&Node<'a>, &AddressMap<'a>) -> Option<T>,
) -> Option<T> {
    visit(dt.root().children(), &AddressMap::new(), &mut f)
}

fn visit<'a, T>(
    nodes: impl Iterator<Item = Node<'a>>,
    map: &AddressMap<'a>,
    f: &mut dyn FnMut(&Node<'a>, &AddressMap<'a>) -> Option<T>,
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

/// Returns the `index`-th `reg` (CPU address, size) of the first enabled node compatible with any of `compatible`.
pub fn find_reg(dt: &DeviceTree, compatible: &[&str], index: usize) -> Option<(u64, u64)> {
    find_map(dt, |node, map| {
        node.is_compatible_with_any(compatible)
            .then(|| map.reg(node, index))
            .flatten()
    })
}

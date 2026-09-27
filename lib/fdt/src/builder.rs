//! Minimal flattened device tree writer for tests
//!
//! Only what the tests of the parser and its users need: nodes, raw
//! properties and the memory reservation block. The blob is 8-byte aligned so
//! that [`crate::DeviceTree::from_slice`] can read it in place.

use alloc::string::String;
use alloc::vec::Vec;

use crate::{DeviceTree, Header};

pub struct FdtBuilder {
    structure: Vec<u8>,
    strings: Vec<u8>,
    reservations: Vec<(u64, u64)>,
    /// Entries written after the terminator are ignored by readers
    raw_reservations: Option<Vec<(u64, u64)>>,
}

impl Default for FdtBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FdtBuilder {
    pub fn new() -> Self {
        Self {
            structure: Vec::new(),
            strings: Vec::new(),
            reservations: Vec::new(),
            raw_reservations: None,
        }
    }

    /// Adds an entry to the memory reservation block.
    pub fn reserve(&mut self, base: u64, size: u64) -> &mut Self {
        self.reservations.push((base, size));
        self
    }

    /// Replaces the memory reservation block, terminator included.
    pub fn raw_reservations(&mut self, entries: &[(u64, u64)]) -> &mut Self {
        self.raw_reservations = Some(entries.to_vec());
        self
    }

    pub fn begin_node(&mut self, name: &str) -> &mut Self {
        self.token(DeviceTree::FDT_BEGIN_NODE);
        self.structure.extend_from_slice(name.as_bytes());
        self.structure.push(0);
        self.pad();
        self
    }

    pub fn end_node(&mut self) -> &mut Self {
        self.token(DeviceTree::FDT_END_NODE);
        self
    }

    pub fn prop(&mut self, name: &str, value: &[u8]) -> &mut Self {
        let offset = self.string_offset(name);
        self.token(DeviceTree::FDT_PROP);
        self.structure
            .extend_from_slice(&(value.len() as u32).to_be_bytes());
        self.structure.extend_from_slice(&offset.to_be_bytes());
        self.structure.extend_from_slice(value);
        self.pad();
        self
    }

    pub fn prop_empty(&mut self, name: &str) -> &mut Self {
        self.prop(name, &[])
    }

    pub fn prop_u32(&mut self, name: &str, value: u32) -> &mut Self {
        self.prop(name, &value.to_be_bytes())
    }

    pub fn prop_cells(&mut self, name: &str, cells: &[u32]) -> &mut Self {
        let bytes: Vec<u8> = cells.iter().flat_map(|v| v.to_be_bytes()).collect();
        self.prop(name, &bytes)
    }

    pub fn prop_str(&mut self, name: &str, value: &str) -> &mut Self {
        let mut bytes = String::from(value).into_bytes();
        bytes.push(0);
        self.prop(name, &bytes)
    }

    /// Returns the blob as 64-bit words, so that it is 8-byte aligned.
    pub fn build(&self) -> Vec<u64> {
        let header_size = core::mem::size_of::<Header>();
        let rsvmap_offset = (header_size + 7) & !7;
        let mut rsvmap = Vec::new();
        let entries = match &self.raw_reservations {
            Some(raw) => raw.clone(),
            None => {
                let mut v = self.reservations.clone();
                v.push((0, 0));
                v
            }
        };
        for (base, size) in entries {
            rsvmap.extend_from_slice(&base.to_be_bytes());
            rsvmap.extend_from_slice(&size.to_be_bytes());
        }
        let mut structure = self.structure.clone();
        structure.extend_from_slice(&DeviceTree::FDT_END.to_be_bytes());
        let struct_offset = rsvmap_offset + rsvmap.len();
        let strings_offset = struct_offset + structure.len();
        let total = strings_offset + self.strings.len();

        let mut blob = Vec::with_capacity(total);
        for value in [
            Header::MAGIC,
            total as u32,
            struct_offset as u32,
            strings_offset as u32,
            rsvmap_offset as u32,
            Header::CURRENT_VERSION,
            Header::COMPATIBLE_VERSION,
            0,
            self.strings.len() as u32,
            structure.len() as u32,
        ] {
            blob.extend_from_slice(&value.to_be_bytes());
        }
        blob.resize(rsvmap_offset, 0);
        blob.extend_from_slice(&rsvmap);
        blob.extend_from_slice(&structure);
        blob.extend_from_slice(&self.strings);

        let mut words = alloc::vec![0u64; total.div_ceil(8)];
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(words.as_mut_ptr() as *mut u8, words.len() * 8)
        };
        bytes[..total].copy_from_slice(&blob);
        words
    }

    fn token(&mut self, token: u32) {
        self.structure.extend_from_slice(&token.to_be_bytes());
    }

    fn pad(&mut self) {
        while self.structure.len() % 4 != 0 {
            self.structure.push(0);
        }
    }

    fn string_offset(&mut self, name: &str) -> u32 {
        let offset = self.strings.len() as u32;
        self.strings.extend_from_slice(name.as_bytes());
        self.strings.push(0);
        offset
    }
}

/// Views a blob built by [`FdtBuilder::build`] as bytes.
pub fn as_bytes(words: &[u64]) -> &[u8] {
    unsafe { core::slice::from_raw_parts(words.as_ptr() as *const u8, words.len() * 8) }
}

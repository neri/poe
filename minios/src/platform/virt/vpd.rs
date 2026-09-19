//! Google VPD (Vital Product Data) 2.0 of Chromebooks
//!
//! Key-value pairs written at the factory, e.g. `region` and `keyboard_layout`.
//! Each entry is a type byte, the key and the value, both preceded by their length
//! (7 bits per byte, big endian, the MSB set on all but the last byte).
//! This module has no hardware dependency.

const VPD_TYPE_STRING: u8 = 0x01;
const VPD_TYPE_INFO: u8 = 0xfe;

/// Keyboard layout decided from the VPD
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardLayout {
    /// US 101-key (also used when the VPD has no information)
    Us,
    /// Japanese 109-key (JIS)
    Japanese,
}

/// Returns the value of `key` in the VPD data.
pub fn find<'a>(data: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut rest = data;
    loop {
        let (&kind, next) = rest.split_first()?;
        if kind != VPD_TYPE_STRING && kind != VPD_TYPE_INFO {
            // Terminator (0x00, 0xff) or unknown type
            return None;
        }
        let (entry_key, next) = read_field(next)?;
        let (value, next) = read_field(next)?;
        if kind == VPD_TYPE_STRING && entry_key == key {
            return Some(value);
        }
        rest = next;
    }
}

/// Decides the keyboard layout, the same way as ChromeOS:
/// `keyboard_layout` (e.g. `xkb:jp::jpn`) if any, otherwise `region` (e.g. `jp`).
pub fn keyboard_layout(data: &[u8]) -> KeyboardLayout {
    let is_japanese = match find(data, b"keyboard_layout") {
        Some(layout) => layout.starts_with(b"xkb:jp:"),
        None => find(data, b"region") == Some(b"jp"),
    };
    if is_japanese {
        KeyboardLayout::Japanese
    } else {
        KeyboardLayout::Us
    }
}

/// Reads a length-prefixed field and returns it with the rest of the data.
fn read_field(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut len = 0usize;
    let mut index = 0;
    loop {
        let byte = *data.get(index)?;
        index += 1;
        len = len.checked_mul(128)? | (byte & 0x7f) as usize;
        if (byte & 0x80) == 0 {
            break;
        }
    }
    let end = index.checked_add(len)?;
    Some((data.get(index..end)?, &data[end..]))
}

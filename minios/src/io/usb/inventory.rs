//! What is plugged into each USB bus, in a form that does not depend on the
//! host controller, and how `lsusb` renders it.
//!
//! Each USB service reports its devices through [`UsbBus`]; the rendering
//! works only from the descriptors the service read while enumerating.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};

use libusb::{DescriptorIter, UsbSpeed};

/// What this system does with a device.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Uses {
    pub hub: bool,
    pub keyboard: bool,
    pub storage: bool,
}

/// One device as a USB service knows it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsbDeviceInfo {
    /// The root port, then the port on each hub on the way down.
    pub path: Vec<u8>,
    pub speed: UsbSpeed,
    /// The USB address, when the host driver assigns it itself.
    pub address: Option<u8>,
    /// The device descriptor, empty until it has been read.
    pub device: Vec<u8>,
    /// The configuration descriptor as far as it was read: the whole of it,
    /// or only its 9-byte header.
    pub configuration: Vec<u8>,
    /// `bConfigurationValue` set on the device, if any.
    pub configured: Option<u8>,
    pub uses: Uses,
}

/// A USB service that can list its devices.
pub trait UsbBus {
    /// Short name of the host controller, such as "xHCI" or "DWC2".
    fn controller(&self) -> &'static str;
    fn devices(&self) -> Vec<UsbDeviceInfo>;
}

/// One bus as `lsusb` sees it, numbered from 1 in registration order.
pub struct BusSnapshot {
    pub number: usize,
    pub controller: &'static str,
    pub devices: Vec<UsbDeviceInfo>,
}

/// Formats `bus-port.port...`, the name a device is given on the command line.
pub fn device_name(bus: usize, path: &[u8]) -> String {
    let mut name = alloc::format!("{}-", bus);
    for (i, port) in path.iter().enumerate() {
        if i > 0 {
            name.push('.');
        }
        let _ = write!(name, "{}", port);
    }
    name
}

/// Finds a device by `bus-port.port...`, or by `port.port...` when there is
/// only one bus.
pub fn find<'a>(
    buses: &'a [BusSnapshot],
    name: &str,
) -> Option<(&'a BusSnapshot, &'a UsbDeviceInfo)> {
    let (bus, ports) = match name.split_once('-') {
        Some((bus, ports)) => (Some(bus.parse::<usize>().ok()?), ports),
        None => (None, name),
    };
    let path = ports
        .split('.')
        .map(|port| port.parse::<u8>().ok())
        .collect::<Option<Vec<u8>>>()?;
    let bus = match bus {
        Some(number) => buses.iter().find(|bus| bus.number == number)?,
        None if buses.len() == 1 => &buses[0],
        None => return None,
    };
    let device = bus.devices.iter().find(|device| device.path == path)?;
    Some((bus, device))
}

/// Writes every bus with its devices as a tree.
pub fn write_tree(out: &mut dyn Write, buses: &[BusSnapshot]) -> fmt::Result {
    for bus in buses {
        writeln!(
            out,
            "Bus {} ({}): {} device(s)",
            bus.number,
            bus.controller,
            bus.devices.len()
        )?;
        let mut roots: Vec<&UsbDeviceInfo> = bus
            .devices
            .iter()
            .filter(|device| {
                let parent = &device.path[..device.path.len().saturating_sub(1)];
                parent.is_empty() || !bus.devices.iter().any(|other| other.path == parent)
            })
            .collect();
        roots.sort_by(|a, b| a.path.cmp(&b.path));
        let count = roots.len();
        for (index, device) in roots.into_iter().enumerate() {
            write_branch(out, bus, device, "", index + 1 == count)?;
        }
    }
    Ok(())
}

fn write_branch(
    out: &mut dyn Write,
    bus: &BusSnapshot,
    device: &UsbDeviceInfo,
    indent: &str,
    last: bool,
) -> fmt::Result {
    write!(out, "{}{}", indent, if last { "`-- " } else { "|-- " })?;
    write_summary(out, bus.number, device)?;
    let mut children: Vec<&UsbDeviceInfo> = bus
        .devices
        .iter()
        .filter(|child| {
            child.path.len() == device.path.len() + 1 && child.path.starts_with(&device.path)
        })
        .collect();
    children.sort_by(|a, b| a.path.cmp(&b.path));
    let indent = alloc::format!("{}{}", indent, if last { "    " } else { "|   " });
    let count = children.len();
    for (index, child) in children.into_iter().enumerate() {
        write_branch(out, bus, child, &indent, index + 1 == count)?;
    }
    Ok(())
}

/// One line: name, IDs, speed, class and what it is used for.
fn write_summary(out: &mut dyn Write, bus: usize, device: &UsbDeviceInfo) -> fmt::Result {
    write!(out, "{} ", device_name(bus, &device.path))?;
    match ids(&device.device) {
        Some((vendor, product)) => write!(out, "{:04x}:{:04x}", vendor, product)?,
        None => write!(out, "????:????")?,
    }
    write!(out, " {}", speed_name(device.speed))?;
    let classes = classes(device);
    if !classes.is_empty() {
        write!(out, " {}", classes)?;
    }
    writeln!(out, " [{}]", uses_name(device.uses))
}

/// Writes everything known about one device, decoding its descriptors.
pub fn write_details(
    out: &mut dyn Write,
    bus: &BusSnapshot,
    device: &UsbDeviceInfo,
) -> fmt::Result {
    writeln!(
        out,
        "Device {} on bus {} ({})",
        device_name(bus.number, &device.path),
        bus.number,
        bus.controller
    )?;
    write!(out, "  {} speed", speed_name(device.speed))?;
    if let Some(address) = device.address {
        write!(out, ", address {}", address)?;
    }
    match device.configured {
        Some(value) => write!(out, ", configuration {} set", value)?,
        None => write!(out, ", not configured")?,
    }
    writeln!(out, ", used as {}", uses_name(device.uses))?;

    let d = &device.device;
    if d.len() < 18 {
        writeln!(out, "  Device descriptor: not read")?;
    } else {
        let bcd_usb = u16::from_le_bytes([d[2], d[3]]);
        let bcd_device = u16::from_le_bytes([d[12], d[13]]);
        writeln!(out, "  Device descriptor")?;
        writeln!(
            out,
            "    bcdUSB {:x}.{:02x}  class {:02x} ({})  subclass {:02x}  protocol {:02x}",
            bcd_usb >> 8,
            bcd_usb & 0xff,
            d[4],
            class_name(d[4]),
            d[5],
            d[6]
        )?;
        writeln!(
            out,
            "    idVendor {:04x}  idProduct {:04x}  bcdDevice {:x}.{:02x}  bMaxPacketSize0 {}",
            u16::from_le_bytes([d[8], d[9]]),
            u16::from_le_bytes([d[10], d[11]]),
            bcd_device >> 8,
            bcd_device & 0xff,
            d[7]
        )?;
        writeln!(
            out,
            "    iManufacturer {}  iProduct {}  iSerialNumber {}  bNumConfigurations {}",
            d[14], d[15], d[16], d[17]
        )?;
    }
    write_configuration(out, device)
}

fn write_configuration(out: &mut dyn Write, device: &UsbDeviceInfo) -> fmt::Result {
    let c = &device.configuration;
    if c.len() < 9 || c[1] != libusb::DESCRIPTOR_CONFIGURATION {
        return writeln!(out, "  Configuration descriptor: not read");
    }
    let total = u16::from_le_bytes([c[2], c[3]]);
    let attributes = c[7];
    // bMaxPower is in 2 mA units, 8 mA at SuperSpeed.
    let unit = if device.speed == UsbSpeed::Super {
        8
    } else {
        2
    };
    write!(
        out,
        "  Configuration {}: {} interface(s), attributes {:02x} ({}",
        c[5],
        c[4],
        attributes,
        if attributes & 0x40 != 0 {
            "self powered"
        } else {
            "bus powered"
        }
    )?;
    if attributes & 0x20 != 0 {
        write!(out, ", remote wakeup")?;
    }
    writeln!(out, "), max power {} mA", c[8] as u32 * unit)?;
    if c.len() < total as usize && c.len() == 9 {
        return writeln!(out, "    (interfaces not read)");
    }
    for item in DescriptorIter::new(c).skip(1) {
        let Ok(item) = item else {
            return writeln!(out, "    (malformed descriptor)");
        };
        let b = item.bytes;
        match item.descriptor_type {
            libusb::DESCRIPTOR_INTERFACE if b.len() >= 9 => writeln!(
                out,
                "    Interface {} alt {}: class {:02x} ({}) subclass {:02x} protocol {:02x}{}, {} endpoint(s)",
                b[2],
                b[3],
                b[5],
                class_name(b[5]),
                b[6],
                b[7],
                interface_detail(b[5], b[6], b[7]),
                b[4]
            )?,
            libusb::DESCRIPTOR_ENDPOINT if b.len() >= 7 => {
                let packet = u16::from_le_bytes([b[4], b[5]]);
                write!(
                    out,
                    "      Endpoint {:#04x} {} {}, max packet {}",
                    b[2],
                    if b[2] & 0x80 != 0 { "IN" } else { "OUT" },
                    ["control", "isochronous", "bulk", "interrupt"][(b[3] & 3) as usize],
                    packet & 0x7ff
                )?;
                if packet >> 11 & 3 != 0 {
                    write!(out, " x{}", (packet >> 11 & 3) + 1)?;
                }
                writeln!(out, ", interval {}", b[6])?
            }
            kind => writeln!(
                out,
                "      Descriptor type {:02x} ({}), {} bytes",
                kind,
                descriptor_name(kind),
                b.len()
            )?,
        }
    }
    Ok(())
}

fn ids(device: &[u8]) -> Option<(u16, u16)> {
    (device.len() >= 12).then(|| {
        (
            u16::from_le_bytes([device[8], device[9]]),
            u16::from_le_bytes([device[10], device[11]]),
        )
    })
}

/// The device class, or the distinct interface classes when the device
/// leaves it to its interfaces.
fn classes(device: &UsbDeviceInfo) -> String {
    let mut names = String::new();
    if device.device.len() >= 5 && device.device[4] != 0 {
        names.push_str(class_name(device.device[4]));
        return names;
    }
    let mut seen: Vec<u8> = Vec::new();
    for item in DescriptorIter::new(&device.configuration).flatten() {
        if item.descriptor_type == libusb::DESCRIPTOR_INTERFACE
            && item.bytes.len() >= 9
            && !seen.contains(&item.bytes[5])
        {
            seen.push(item.bytes[5]);
            if !names.is_empty() {
                names.push('+');
            }
            names.push_str(class_name(item.bytes[5]));
        }
    }
    names
}

fn uses_name(uses: Uses) -> &'static str {
    match (uses.hub, uses.keyboard, uses.storage) {
        (true, _, _) => "hub",
        (false, true, true) => "keyboard, storage",
        (false, true, false) => "keyboard",
        (false, false, true) => "storage",
        (false, false, false) => "not used",
    }
}

fn speed_name(speed: UsbSpeed) -> &'static str {
    match speed {
        UsbSpeed::Low => "Low",
        UsbSpeed::Full => "Full",
        UsbSpeed::High => "High",
        UsbSpeed::Super => "Super",
    }
}

fn class_name(class: u8) -> &'static str {
    match class {
        0x00 => "per interface",
        0x01 => "Audio",
        0x02 => "Communications",
        0x03 => "HID",
        0x05 => "Physical",
        0x06 => "Image",
        0x07 => "Printer",
        0x08 => "Mass storage",
        0x09 => "Hub",
        0x0a => "CDC data",
        0x0b => "Smart card",
        0x0d => "Content security",
        0x0e => "Video",
        0x0f => "Healthcare",
        0x10 => "Audio/Video",
        0x11 => "Billboard",
        0xdc => "Diagnostic",
        0xe0 => "Wireless",
        0xef => "Miscellaneous",
        0xfe => "Application specific",
        0xff => "Vendor specific",
        _ => "unknown",
    }
}

/// The meaning of the subclass and protocol for the classes this system uses.
fn interface_detail(class: u8, subclass: u8, protocol: u8) -> &'static str {
    match (class, subclass, protocol) {
        (0x03, 0x01, 0x01) => " boot keyboard",
        (0x03, 0x01, 0x02) => " boot mouse",
        (0x08, 0x06, 0x50) => " SCSI, bulk-only",
        (0x09, _, 0x00) => " single TT or full speed",
        (0x09, _, 0x01) => " single TT",
        (0x09, _, 0x02) => " multiple TT",
        _ => "",
    }
}

fn descriptor_name(kind: u8) -> &'static str {
    match kind {
        0x0b => "interface association",
        0x21 => "HID",
        0x24 => "class-specific interface",
        0x25 => "class-specific endpoint",
        0x29 => "hub",
        0x30 => "SuperSpeed endpoint companion",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    const HUB: [u8; 18] = [
        18, 1, 0x00, 0x02, 0x09, 0x00, 0x01, 64, 0x40, 0x1a, 0x01, 0x01, 0x11, 0x01, 0, 1, 0, 1,
    ];
    const KEYBOARD: [u8; 18] = [
        18, 1, 0x10, 0x01, 0x00, 0x00, 0x00, 8, 0x6d, 0x04, 0x1c, 0xc3, 0x02, 0x64, 1, 2, 0, 1,
    ];
    const KEYBOARD_CONFIGURATION: [u8; 34] = [
        9, 2, 34, 0, 1, 1, 0, 0xa0, 50, // configuration
        9, 4, 0, 0, 1, 3, 1, 1, 0, // interface
        9, 0x21, 0x10, 0x01, 0, 1, 0x22, 63, 0, // HID
        7, 5, 0x81, 3, 8, 0, 10, // endpoint
    ];

    fn bus() -> BusSnapshot {
        BusSnapshot {
            number: 1,
            controller: "DWC2",
            devices: vec![
                UsbDeviceInfo {
                    path: vec![1, 3],
                    speed: UsbSpeed::Low,
                    address: Some(2),
                    device: KEYBOARD.to_vec(),
                    configuration: KEYBOARD_CONFIGURATION.to_vec(),
                    configured: Some(1),
                    uses: Uses {
                        keyboard: true,
                        ..Uses::default()
                    },
                },
                UsbDeviceInfo {
                    path: vec![1],
                    speed: UsbSpeed::High,
                    address: Some(1),
                    device: HUB.to_vec(),
                    configuration: vec![9, 2, 25, 0, 1, 1, 0, 0xe0, 50],
                    configured: Some(1),
                    uses: Uses {
                        hub: true,
                        ..Uses::default()
                    },
                },
                UsbDeviceInfo {
                    path: vec![1, 1],
                    speed: UsbSpeed::High,
                    address: None,
                    device: Vec::new(),
                    configuration: Vec::new(),
                    configured: None,
                    uses: Uses::default(),
                },
            ],
        }
    }

    #[test]
    fn tree_nests_devices_under_their_hub() {
        let mut text = String::new();
        write_tree(&mut text, &[bus()]).unwrap();
        assert_eq!(
            text,
            "Bus 1 (DWC2): 3 device(s)\n\
             `-- 1-1 1a40:0101 High Hub [hub]\n    \
             |-- 1-1.1 ????:???? High [not used]\n    \
             `-- 1-1.3 046d:c31c Low HID [keyboard]\n"
        );
    }

    #[test]
    fn find_accepts_a_path_with_or_without_the_bus() {
        let buses = [bus()];
        assert_eq!(find(&buses, "1-1.3").unwrap().1.address, Some(2));
        assert_eq!(find(&buses, "1.3").unwrap().1.address, Some(2));
        assert!(find(&buses, "2-1").is_none());
        assert!(find(&buses, "1.x").is_none());
    }

    #[test]
    fn details_decode_the_descriptors() {
        let buses = [bus()];
        let (bus, device) = find(&buses, "1-1.3").unwrap();
        let mut text = String::new();
        write_details(&mut text, bus, device).unwrap();
        assert!(text.contains("Device 1-1.3 on bus 1 (DWC2)"), "{text}");
        assert!(
            text.contains("idVendor 046d  idProduct c31c  bcdDevice 64.02"),
            "{text}"
        );
        assert!(text.contains("max power 100 mA"), "{text}");
        assert!(
            text.contains(
                "Interface 0 alt 0: class 03 (HID) subclass 01 protocol 01 boot keyboard"
            ),
            "{text}"
        );
        assert!(
            text.contains("Endpoint 0x81 IN interrupt, max packet 8, interval 10"),
            "{text}"
        );
        assert!(text.contains("Descriptor type 21 (HID), 9 bytes"), "{text}");

        let (bus, hub) = find(&buses, "1-1").unwrap();
        text.clear();
        write_details(&mut text, bus, hub).unwrap();
        assert!(text.contains("(interfaces not read)"), "{text}");
    }
}

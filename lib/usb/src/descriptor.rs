use crate::{EndpointAddress, TransferType, UsbError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Descriptor<'a> {
    pub descriptor_type: u8,
    pub bytes: &'a [u8],
}

pub struct DescriptorIter<'a> {
    bytes: &'a [u8],
    failed: bool,
}

impl<'a> DescriptorIter<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            failed: false,
        }
    }
}

impl<'a> Iterator for DescriptorIter<'a> {
    type Item = Result<Descriptor<'a>, UsbError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() || self.failed {
            return None;
        }
        if self.bytes.len() < 2 {
            self.failed = true;
            return Some(Err(UsbError::InvalidDescriptor));
        }
        let length = self.bytes[0] as usize;
        if length < 2 || length > self.bytes.len() {
            self.failed = true;
            return Some(Err(UsbError::InvalidDescriptor));
        }
        let (item, rest) = self.bytes.split_at(length);
        self.bytes = rest;
        Some(Ok(Descriptor {
            descriptor_type: item[1],
            bytes: item,
        }))
    }
}

fn le16(bytes: &[u8], offset: usize) -> Result<u16, UsbError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(UsbError::InvalidDescriptor)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceDescriptor {
    pub usb: u16,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    /// `bMaxPacketSize0` as sent: a byte count, except from a USB 3.x device
    /// running at SuperSpeed, where it is the exponent 9 (512 bytes).
    pub max_packet_size_0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device: u16,
    pub configurations: u8,
}
impl DeviceDescriptor {
    pub fn parse(b: &[u8]) -> Result<Self, UsbError> {
        if b.len() < 18 || b[0] != 18 || b[1] != 1 {
            return Err(UsbError::InvalidDescriptor);
        }
        // 9 is only meaningful from a USB 3.x device; whether it fits the
        // speed the device is running at is for the host controller to judge.
        let superspeed = le16(b, 2)? >= 0x0300 && b[7] == 9;
        if !superspeed && !matches!(b[7], 8 | 16 | 32 | 64) {
            return Err(UsbError::InvalidDescriptor);
        }
        Ok(Self {
            usb: le16(b, 2)?,
            class: b[4],
            subclass: b[5],
            protocol: b[6],
            max_packet_size_0: b[7],
            vendor_id: le16(b, 8)?,
            product_id: le16(b, 10)?,
            device: le16(b, 12)?,
            configurations: b[17],
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationDescriptor {
    pub total_length: u16,
    pub interfaces: u8,
    pub value: u8,
    pub attributes: u8,
    pub max_power: u8,
}
impl ConfigurationDescriptor {
    pub fn parse(b: &[u8]) -> Result<Self, UsbError> {
        if b.len() < 9 || b[0] != 9 || b[1] != 2 {
            return Err(UsbError::InvalidDescriptor);
        }
        let total_length = le16(b, 2)?;
        if total_length < 9 {
            return Err(UsbError::InvalidDescriptor);
        }
        Ok(Self {
            total_length,
            interfaces: b[4],
            value: b[5],
            attributes: b[7],
            max_power: b[8],
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterfaceDescriptor {
    pub number: u8,
    pub alternate: u8,
    pub endpoints: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
}
impl InterfaceDescriptor {
    pub fn parse(b: &[u8]) -> Result<Self, UsbError> {
        if b.len() < 9 || b[0] != 9 || b[1] != 4 {
            return Err(UsbError::InvalidDescriptor);
        }
        Ok(Self {
            number: b[2],
            alternate: b[3],
            endpoints: b[4],
            class: b[5],
            subclass: b[6],
            protocol: b[7],
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EndpointDescriptor {
    pub address: EndpointAddress,
    pub transfer_type: TransferType,
    pub max_packet_size: u16,
    pub interval: u8,
}
impl EndpointDescriptor {
    pub fn parse(b: &[u8]) -> Result<Self, UsbError> {
        if b.len() < 7 || b[0] != 7 || b[1] != 5 {
            return Err(UsbError::InvalidDescriptor);
        }
        let address = EndpointAddress::from_raw(b[2]).ok_or(UsbError::InvalidDescriptor)?;
        let max_packet_size = le16(b, 4)? & 0x07ff;
        if max_packet_size == 0 {
            return Err(UsbError::InvalidDescriptor);
        }
        Ok(Self {
            address,
            transfer_type: TransferType::from_attributes(b[3]),
            max_packet_size,
            interval: b[6],
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HubDescriptor {
    pub ports: u8,
    pub characteristics: u16,
    pub power_on_to_good_2ms: u8,
}
impl HubDescriptor {
    pub fn parse(b: &[u8]) -> Result<Self, UsbError> {
        if b.len() < 7 || b[0] as usize > b.len() || b[1] != 0x29 || b[2] == 0 {
            return Err(UsbError::InvalidDescriptor);
        }
        Ok(Self {
            ports: b[2],
            characteristics: le16(b, 3)?,
            power_on_to_good_2ms: b[5],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Direction;
    #[test]
    fn rejects_zero_and_truncated_lengths() {
        assert!(DescriptorIter::new(&[0, 1]).next().unwrap().is_err());
        assert!(DescriptorIter::new(&[4, 1, 0]).next().unwrap().is_err());
    }
    #[test]
    fn retains_unknown_descriptors() {
        let b = [3, 0x99, 7, 4, 2, 9, 8];
        let v: Vec<_> = DescriptorIter::new(&b).collect();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].as_ref().unwrap().descriptor_type, 0x99);
    }
    #[test]
    fn iterator_stops_after_a_malformed_item() {
        // A valid interface descriptor followed by a length that runs past
        // the end: the good one is returned, then the error, then nothing.
        let b = [9, 4, 0, 0, 1, 3, 1, 1, 0, 9, 5, 0x81];
        let v: Vec<_> = DescriptorIter::new(&b).collect();
        assert_eq!(v.len(), 2);
        assert!(v[0].is_ok());
        assert_eq!(v[1], Err(UsbError::InvalidDescriptor));
    }
    #[test]
    fn empty_input_yields_nothing() {
        assert_eq!(DescriptorIter::new(&[]).count(), 0);
    }
    #[test]
    fn single_trailing_byte_is_an_error() {
        assert_eq!(
            DescriptorIter::new(&[2]).next(),
            Some(Err(UsbError::InvalidDescriptor))
        );
    }
    #[test]
    fn parses_a_device_descriptor() {
        let b = [
            18, 1, 0x00, 0x02, 0, 0, 0, 8, 0x4f, 0x1c, 0x27, 0x00, 0x10, 0x01, 1, 2, 0, 1,
        ];
        let d = DeviceDescriptor::parse(&b).unwrap();
        assert_eq!(d.usb, 0x0200);
        assert_eq!(d.max_packet_size_0, 8);
        assert_eq!(d.vendor_id, 0x1c4f);
        assert_eq!(d.product_id, 0x0027);
        assert_eq!(d.configurations, 1);
    }
    #[test]
    fn a_superspeed_device_gives_ep0_as_an_exponent() {
        let mut b = [
            18, 1, 0x20, 0x03, 0, 0, 0, 9, 0x4f, 0x1c, 0x27, 0x00, 0x10, 0x01, 1, 2, 0, 1,
        ];
        assert_eq!(DeviceDescriptor::parse(&b).unwrap().max_packet_size_0, 9);
        b[3] = 0x02;
        assert!(
            DeviceDescriptor::parse(&b).is_err(),
            "9 from a USB 2.0 device"
        );
    }
    #[test]
    fn rejects_bad_device_descriptors() {
        let good = [
            18, 1, 0, 2, 0, 0, 0, 8, 0x4f, 0x1c, 0x27, 0, 0x10, 1, 1, 2, 0, 1,
        ];
        assert!(DeviceDescriptor::parse(&good[..17]).is_err(), "truncated");
        let mut b = good;
        b[0] = 17;
        assert!(DeviceDescriptor::parse(&b).is_err(), "length byte");
        let mut b = good;
        b[1] = 2;
        assert!(DeviceDescriptor::parse(&b).is_err(), "type byte");
        for size in [0u8, 1, 9, 63, 128] {
            let mut b = good;
            b[7] = size;
            assert!(
                DeviceDescriptor::parse(&b).is_err(),
                "endpoint zero size {size}"
            );
        }
    }
    #[test]
    fn rejects_a_configuration_shorter_than_its_own_header() {
        let mut b = [9, 2, 9, 0, 1, 1, 0, 0x80, 50];
        assert_eq!(ConfigurationDescriptor::parse(&b).unwrap().total_length, 9);
        b[2] = 8;
        assert!(ConfigurationDescriptor::parse(&b).is_err());
        b[2] = 0;
        assert!(ConfigurationDescriptor::parse(&b).is_err());
    }
    #[test]
    fn parses_an_endpoint_and_masks_the_packet_size() {
        // Bits 11 and 12 carry the transactions-per-microframe field.
        let b = [7, 5, 0x81, 3, 0x08, 0x18, 10];
        let e = EndpointDescriptor::parse(&b).unwrap();
        assert_eq!(e.address.number(), 1);
        assert_eq!(e.address.direction(), Direction::In);
        assert_eq!(e.transfer_type, TransferType::Interrupt);
        assert_eq!(e.max_packet_size, 8);
        assert_eq!(e.interval, 10);
    }
    #[test]
    fn rejects_bad_endpoints() {
        let good = [7, 5, 0x81, 3, 8, 0, 10];
        assert!(EndpointDescriptor::parse(&good[..6]).is_err(), "truncated");
        let mut b = good;
        b[4] = 0;
        assert!(EndpointDescriptor::parse(&b).is_err(), "zero packet size");
        let mut b = good;
        b[2] = 0x30;
        assert!(EndpointDescriptor::parse(&b).is_err(), "reserved address");
    }
    #[test]
    fn parses_a_hub_descriptor() {
        let b = [9, 0x29, 5, 0x09, 0x00, 50, 10, 0, 0xff];
        let h = HubDescriptor::parse(&b).unwrap();
        assert_eq!(h.ports, 5);
        assert_eq!(h.characteristics, 0x0009);
        assert_eq!(h.power_on_to_good_2ms, 50);
        let mut b = b;
        b[2] = 0;
        assert!(HubDescriptor::parse(&b).is_err(), "no ports");
        let mut b = b;
        b[1] = 0x2a;
        assert!(HubDescriptor::parse(&b).is_err(), "wrong type");
    }
    #[test]
    fn parses_composite_chain() {
        let b = [9, 2, 18, 0, 1, 1, 0, 0x80, 50, 9, 4, 0, 0, 0, 3, 1, 1, 0];
        let v: Vec<_> = DescriptorIter::new(&b).collect();
        assert_eq!(v.len(), 2);
        assert_eq!(
            ConfigurationDescriptor::parse(v[0].as_ref().unwrap().bytes)
                .unwrap()
                .total_length,
            18
        );
        assert_eq!(
            InterfaceDescriptor::parse(v[1].as_ref().unwrap().bytes)
                .unwrap()
                .class,
            3
        );
    }
}

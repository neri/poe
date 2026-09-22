use crate::Direction;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct SetupPacket {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

impl SetupPacket {
    pub const SIZE: usize = 8;
    pub const fn new(request_type: u8, request: u8, value: u16, index: u16, length: u16) -> Self {
        Self {
            request_type,
            request,
            value,
            index,
            length,
        }
    }
    pub const fn direction(self) -> Direction {
        if self.request_type & 0x80 != 0 {
            Direction::In
        } else {
            Direction::Out
        }
    }
    pub const fn to_bytes(self) -> [u8; 8] {
        let value = self.value.to_le_bytes();
        let index = self.index.to_le_bytes();
        let length = self.length.to_le_bytes();
        [
            self.request_type,
            self.request,
            value[0],
            value[1],
            index[0],
            index[1],
            length[0],
            length[1],
        ]
    }
    pub const fn get_descriptor(kind: u8, index: u8, language: u16, length: u16) -> Self {
        Self::new(
            0x80,
            StandardRequest::GetDescriptor as u8,
            ((kind as u16) << 8) | index as u16,
            language,
            length,
        )
    }
    pub const fn set_address(address: u8) -> Self {
        Self::new(0, StandardRequest::SetAddress as u8, address as u16, 0, 0)
    }
    pub const fn set_configuration(configuration: u8) -> Self {
        Self::new(
            0,
            StandardRequest::SetConfiguration as u8,
            configuration as u16,
            0,
            0,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StandardRequest {
    GetStatus = 0,
    ClearFeature = 1,
    SetFeature = 3,
    SetAddress = 5,
    GetDescriptor = 6,
    SetDescriptor = 7,
    GetConfiguration = 8,
    SetConfiguration = 9,
    GetInterface = 10,
    SetInterface = 11,
    SynchFrame = 12,
}

pub const DESCRIPTOR_DEVICE: u8 = 1;
pub const DESCRIPTOR_CONFIGURATION: u8 = 2;
pub const DESCRIPTOR_STRING: u8 = 3;
pub const DESCRIPTOR_INTERFACE: u8 = 4;
pub const DESCRIPTOR_ENDPOINT: u8 = 5;
pub const DESCRIPTOR_HID: u8 = 0x21;
pub const DESCRIPTOR_HUB: u8 = 0x29;
pub const DESCRIPTOR_SS_ENDPOINT_COMPANION: u8 = 0x30;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_requests_round_trip() {
        assert_eq!(
            SetupPacket::get_descriptor(DESCRIPTOR_CONFIGURATION, 0, 0, 59).to_bytes(),
            [0x80, 6, 0x00, 0x02, 0, 0, 59, 0]
        );
        assert_eq!(
            SetupPacket::set_address(3).to_bytes(),
            [0x00, 5, 3, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            SetupPacket::set_configuration(1).to_bytes(),
            [0x00, 9, 1, 0, 0, 0, 0, 0]
        );
    }
    #[test]
    fn direction_comes_from_the_request_type() {
        assert_eq!(
            SetupPacket::get_descriptor(DESCRIPTOR_DEVICE, 0, 0, 18).direction(),
            Direction::In
        );
        assert_eq!(SetupPacket::set_address(1).direction(), Direction::Out);
    }
    #[test]
    fn setup_packet_layout_is_little_endian() {
        assert_eq!(
            SetupPacket::new(0x81, 6, 0x1234, 0x5678, 0x9abc).to_bytes(),
            [0x81, 6, 0x34, 0x12, 0x78, 0x56, 0xbc, 0x9a]
        );
    }
}

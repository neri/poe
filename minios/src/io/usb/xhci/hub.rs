//! USB 2.0 hub class requests and status decoding.
//!
//! Only what one tier of hub needs, per `docs/USB_HOST_RPI4_PLAN.md`: power a
//! downstream port, read its status, reset it, and clear the change bits.
//! Nothing here talks to a controller, so all of it is testable on the host.
//!
//! This is separate from [`crate::io::usb::class::hub`], which carries the
//! DWC2 stack's split-transaction routing in its types.  The requests are the
//! same USB; the surrounding model is not.

use libusb::{SetupPacket, UsbError, UsbSpeed};

/// `bDeviceClass` / `bInterfaceClass` of a hub.
pub const CLASS_HUB: u8 = 9;

/// Descriptor type of a hub descriptor.
pub const DESCRIPTOR_HUB: u8 = 0x29;

/// Downstream ports this driver is willing to scan on one hub.
pub const MAX_DOWNSTREAM_PORTS: u8 = 15;

/// Hub port feature selectors.
pub mod feature {
    pub const PORT_ENABLE: u16 = 1;
    pub const PORT_RESET: u16 = 4;
    pub const PORT_POWER: u16 = 8;
    pub const C_PORT_CONNECTION: u16 = 16;
    pub const C_PORT_ENABLE: u16 = 17;
    pub const C_PORT_SUSPEND: u16 = 18;
    pub const C_PORT_OVER_CURRENT: u16 = 19;
    pub const C_PORT_RESET: u16 = 20;
}

/// `GET_DESCRIPTOR` for the hub descriptor: class request, device recipient.
pub const fn get_hub_descriptor(length: u16) -> SetupPacket {
    SetupPacket::new(0xa0, 0x06, (DESCRIPTOR_HUB as u16) << 8, 0, length)
}

/// `GET_STATUS` of a downstream port: class request, other recipient.
pub const fn get_port_status(port: u8) -> SetupPacket {
    SetupPacket::new(0xa3, 0x00, 0, port as u16, 4)
}

/// `SET_FEATURE` on a downstream port.
pub const fn set_port_feature(port: u8, feature: u16) -> SetupPacket {
    SetupPacket::new(0x23, 0x03, feature, port as u16, 0)
}

/// `CLEAR_FEATURE` on a downstream port.
pub const fn clear_port_feature(port: u8, feature: u16) -> SetupPacket {
    SetupPacket::new(0x23, 0x01, feature, port as u16, 0)
}

/// The fields of a hub descriptor this driver uses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HubDescriptor {
    pub ports: u8,
    /// `TT Think Time`, already reduced to the two-bit field a Slot Context
    /// wants: 0 = 8 FS bit times, 1 = 16, 2 = 24, 3 = 32.
    pub tt_think_time: u8,
    /// How long after switching a port on its power is good, in microseconds.
    pub power_on_delay_us: u64,
}

impl HubDescriptor {
    pub fn parse(bytes: &[u8]) -> Result<Self, UsbError> {
        // bDescLength, bDescriptorType, bNbrPorts, wHubCharacteristics(2),
        // bPwrOn2PwrGood, bHubContrCurrent
        if bytes.len() < 7 || bytes[1] != DESCRIPTOR_HUB || (bytes[0] as usize) < 7 {
            return Err(UsbError::InvalidDescriptor);
        }
        let characteristics = u16::from_le_bytes([bytes[3], bytes[4]]);
        Ok(Self {
            ports: bytes[2].min(MAX_DOWNSTREAM_PORTS),
            tt_think_time: ((characteristics >> 5) & 3) as u8,
            // `bPwrOn2PwrGood` is in 2 ms units.  The specification also asks
            // for 100 ms of settling before a device behind a freshly powered
            // port is believed, so the larger of the two is what matters.
            power_on_delay_us: (bytes[5] as u64 * 2_000).max(100_000),
        })
    }
}

/// The status of one downstream port.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PortStatus {
    pub connected: bool,
    pub enabled: bool,
    pub suspended: bool,
    pub over_current: bool,
    pub resetting: bool,
    pub powered: bool,
    /// `None` while the port is not enabled, because the speed bits are only
    /// meaningful after a reset has completed.
    pub speed: Option<UsbSpeed>,
    pub connection_changed: bool,
    pub enable_changed: bool,
    pub suspend_changed: bool,
    pub over_current_changed: bool,
    pub reset_changed: bool,
    pub raw_status: u16,
    pub raw_change: u16,
}

impl PortStatus {
    pub fn parse(bytes: &[u8]) -> Result<Self, UsbError> {
        if bytes.len() < 4 {
            return Err(UsbError::InvalidDescriptor);
        }
        let status = u16::from_le_bytes([bytes[0], bytes[1]]);
        let change = u16::from_le_bytes([bytes[2], bytes[3]]);
        let enabled = status & (1 << 1) != 0;
        Ok(Self {
            connected: status & 1 != 0,
            enabled,
            suspended: status & (1 << 2) != 0,
            over_current: status & (1 << 3) != 0,
            resetting: status & (1 << 4) != 0,
            powered: status & (1 << 8) != 0,
            // Low Speed is bit 9 and High Speed bit 10; neither set means
            // Full Speed.  The pair is only valid once the port is enabled.
            speed: enabled.then(|| {
                if status & (1 << 9) != 0 {
                    UsbSpeed::Low
                } else if status & (1 << 10) != 0 {
                    UsbSpeed::High
                } else {
                    UsbSpeed::Full
                }
            }),
            connection_changed: change & 1 != 0,
            enable_changed: change & (1 << 1) != 0,
            suspend_changed: change & (1 << 2) != 0,
            over_current_changed: change & (1 << 3) != 0,
            reset_changed: change & (1 << 4) != 0,
            raw_status: status,
            raw_change: change,
        })
    }

    /// The feature selectors that have to be cleared to acknowledge this
    /// port's change bits, in the order they should be sent.
    pub fn pending_change_features(&self) -> impl Iterator<Item = u16> + '_ {
        [
            (self.connection_changed, feature::C_PORT_CONNECTION),
            (self.enable_changed, feature::C_PORT_ENABLE),
            (self.suspend_changed, feature::C_PORT_SUSPEND),
            (self.over_current_changed, feature::C_PORT_OVER_CURRENT),
            (self.reset_changed, feature::C_PORT_RESET),
        ]
        .into_iter()
        .filter_map(|(set, selector)| set.then_some(selector))
    }
}

/// The Route String for a device one tier below the root.
///
/// A Route String holds four bits per tier, the first tier in the low bits.
/// A port number above 15 cannot be expressed, which is the real reason
/// [`MAX_DOWNSTREAM_PORTS`] stops where it does.
#[inline]
pub const fn route_string_for(port: u8) -> u32 {
    (port as u32) & 0xf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_class_requests_address_the_port_not_the_device() {
        let status = get_port_status(3);
        let bytes = status.to_bytes();
        // bmRequestType, bRequest, wValue, wIndex, wLength
        assert_eq!(bytes[0], 0xa3, "device-to-host, class, other recipient");
        assert_eq!(bytes[1], 0x00, "GET_STATUS");
        assert_eq!(
            u16::from_le_bytes([bytes[4], bytes[5]]),
            3,
            "wIndex is the port"
        );
        assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), 4);

        let reset = set_port_feature(2, feature::PORT_RESET);
        let bytes = reset.to_bytes();
        assert_eq!(bytes[0], 0x23, "host-to-device, class, other recipient");
        assert_eq!(bytes[1], 0x03, "SET_FEATURE");
        assert_eq!(
            u16::from_le_bytes([bytes[2], bytes[3]]),
            feature::PORT_RESET
        );
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 2);
    }

    #[test]
    fn a_hub_descriptor_yields_ports_and_think_time() {
        // 9 bytes, 4 ports, characteristics with TT think time = 1 (bits 6:5),
        // bPwrOn2PwrGood = 50 (100 ms).
        let bytes = [9, DESCRIPTOR_HUB, 4, 0x20, 0x00, 50, 0, 0, 0];
        let hub = HubDescriptor::parse(&bytes).unwrap();
        assert_eq!(hub.ports, 4);
        assert_eq!(hub.tt_think_time, 1);
        assert_eq!(hub.power_on_delay_us, 100_000);
    }

    #[test]
    fn a_short_power_on_delay_still_gets_the_settling_time() {
        // bPwrOn2PwrGood of 1 is 2 ms, far less than the 100 ms of connection
        // settling the specification asks for before believing a device.
        let bytes = [9, DESCRIPTOR_HUB, 2, 0, 0, 1, 0, 0, 0];
        assert_eq!(
            HubDescriptor::parse(&bytes).unwrap().power_on_delay_us,
            100_000
        );
    }

    #[test]
    fn a_descriptor_of_the_wrong_type_is_rejected() {
        assert_eq!(
            HubDescriptor::parse(&[9, 0x21, 4, 0, 0, 50, 0, 0, 0]),
            Err(UsbError::InvalidDescriptor)
        );
        assert_eq!(HubDescriptor::parse(&[9]), Err(UsbError::InvalidDescriptor));
    }

    #[test]
    fn port_speed_is_only_read_once_the_port_is_enabled() {
        // Connected and low-speed bit set, but not yet enabled: the speed bits
        // mean nothing until the reset has finished.
        let connected = PortStatus::parse(&[0x01, 0x02, 0x01, 0x00]).unwrap();
        assert!(connected.connected);
        assert!(!connected.enabled);
        assert_eq!(connected.speed, None);
        assert!(connected.connection_changed);

        let low = PortStatus::parse(&[0x03, 0x02, 0x10, 0x00]).unwrap();
        assert!(low.enabled);
        assert_eq!(low.speed, Some(UsbSpeed::Low));
        assert!(low.reset_changed);

        let high = PortStatus::parse(&[0x03, 0x04, 0x00, 0x00]).unwrap();
        assert_eq!(high.speed, Some(UsbSpeed::High));

        let full = PortStatus::parse(&[0x03, 0x00, 0x00, 0x00]).unwrap();
        assert_eq!(full.speed, Some(UsbSpeed::Full));
    }

    #[test]
    fn only_the_change_bits_that_are_set_are_acknowledged() {
        let status = PortStatus::parse(&[0x01, 0x00, 0x11, 0x00]).unwrap();
        let features: alloc::vec::Vec<u16> = status.pending_change_features().collect();
        assert_eq!(
            features,
            [feature::C_PORT_CONNECTION, feature::C_PORT_RESET]
        );

        let quiet = PortStatus::parse(&[0x01, 0x00, 0x00, 0x00]).unwrap();
        assert_eq!(quiet.pending_change_features().count(), 0);
    }

    #[test]
    fn a_route_string_holds_one_tier_in_its_low_nibble() {
        assert_eq!(route_string_for(1), 1);
        assert_eq!(route_string_for(15), 15);
        // Ports past 15 cannot be routed, which is why they are never scanned.
        assert_eq!(route_string_for(16), 0);
    }
}

use libusb::{UsbAddress, UsbSpeed};

use crate::io::usb::hcd::SplitTarget;

pub const CLASS_HUB: u8 = 9;
pub const MAX_DOWNSTREAM_PORTS: u8 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortPhase {
    Disconnected,
    Debouncing { until_us: u64 },
    Resetting { until_us: u64 },
    Active(UsbSpeed),
    Failed,
}

pub struct HubPort {
    pub number: u8,
    pub phase: PortPhase,
}
impl HubPort {
    pub const fn new(number: u8) -> Self {
        Self {
            number,
            phase: PortPhase::Disconnected,
        }
    }
    pub fn connection_changed(&mut self, connected: bool, now_us: u64) {
        self.phase = if connected {
            PortPhase::Debouncing {
                until_us: now_us.saturating_add(100_000),
            }
        } else {
            PortPhase::Disconnected
        }
    }
    pub fn poll(&mut self, now_us: u64) {
        if let PortPhase::Debouncing { until_us } = self.phase
            && now_us >= until_us
        {
            self.phase = PortPhase::Resetting {
                until_us: now_us.saturating_add(50_000),
            }
        }
    }
}

pub const fn child_split_route(
    hub_address: UsbAddress,
    port_number: u8,
    child_speed: UsbSpeed,
) -> Option<SplitTarget> {
    match child_speed {
        UsbSpeed::Low | UsbSpeed::Full => Some(SplitTarget {
            hub_address,
            port_number,
            hub_speed: UsbSpeed::High,
        }),
        UsbSpeed::High | UsbSpeed::Super => None,
    }
}

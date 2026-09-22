//! USB mass storage, Bulk-Only Transport, read-only.
//!
//! See `docs/USB_MSC_RPI_PLAN.md`.  The layers, top down:
//!
//! * [`registry`] — the table the USB services publish devices in, and the
//!   synchronous, read-only [`registry::UsbBlockDevice`] over it.
//! * [`session`] — SCSI: readiness, capacity, sense, chunked reads.
//! * [`bot`] — Bulk-Only Transport: CBW, data, CSW and Reset Recovery.
//! * [`wire`] and [`scsi`] — the byte formats.
//!
//! None of it knows which controller it runs on.  The DWC2 manager and the
//! xHCI service each find the interfaces with [`find_interfaces`], create a
//! [`session::MscSession`] per interface, and carry out the transfers it asks
//! for.

use alloc::vec::Vec;

use libusb::{
    ConfigurationDescriptor, DescriptorIter, Direction, EndpointDescriptor, InterfaceDescriptor,
    TransferType, UsbError,
};

pub mod bot;
pub mod registry;
pub mod scsi;
pub mod session;
pub mod wire;

#[cfg(test)]
pub(crate) mod fake_disk;
#[cfg(test)]
mod tests;

pub use registry::{DeviceSummary, MediaState, UsbBlockDevice, devices, now_us, open};
pub use session::MscSession;

/// Mass storage interfaces taken from one device.
pub const MAX_INTERFACES_PER_DEVICE: usize = 4;

/// A BOT interface and its two bulk endpoints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MscInterface {
    pub configuration: u8,
    pub number: u8,
    pub bulk_in: EndpointDescriptor,
    pub bulk_out: EndpointDescriptor,
    /// `bMaxBurst` of each endpoint's SuperSpeed Endpoint Companion: how many
    /// packets beyond the first it may move per burst.  Zero for a device
    /// that is not running at SuperSpeed, which has no companions.
    pub burst_in: u8,
    pub burst_out: u8,
}

/// A mass storage interface that was found and not taken, and why.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rejected {
    pub number: u8,
    pub reason: &'static str,
}

/// What a configuration descriptor holds for this driver.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Found {
    pub interfaces: Vec<MscInterface>,
    pub rejected: Vec<Rejected>,
}

/// A bulk endpoint's packet size has to be one USB allows: 8 to 64 at Full
/// Speed, 512 at High Speed, 1024 at SuperSpeed.
const fn bulk_packet_size_valid(size: u16) -> bool {
    matches!(size, 8 | 16 | 32 | 64 | 512 | 1024)
}

/// The largest `bMaxBurst` a SuperSpeed Endpoint Companion may carry.
const MAX_BURST: u8 = 15;

#[derive(Clone, Copy)]
struct Candidate {
    number: u8,
    bulk_in: Option<EndpointDescriptor>,
    bulk_out: Option<EndpointDescriptor>,
    burst_in: u8,
    burst_out: u8,
    /// The bulk endpoint a companion descriptor would belong to: the one
    /// just taken, until anything else comes between.
    last: Option<Direction>,
    problem: Option<&'static str>,
}

/// Finds the SCSI/BOT interfaces of a configuration.
///
/// Only alternate setting 0 is considered: a UAS device keeps BOT there, and
/// selecting another setting would mean a SET_INTERFACE this driver does not
/// issue.  A mass storage interface that is not BOT, or whose endpoints are
/// missing, duplicated or malformed, is listed in [`Found::rejected`] rather
/// than silently ignored.  A malformed descriptor chain is an error.
pub fn find_interfaces(config: &[u8]) -> Result<Found, UsbError> {
    let configuration =
        ConfigurationDescriptor::parse(config.get(..9).ok_or(UsbError::InvalidDescriptor)?)?;
    let mut found = Found::default();
    let mut current: Option<Candidate> = None;
    let finish = |candidate: Option<Candidate>, found: &mut Found| {
        let Some(c) = candidate else { return };
        let reason = match (c.problem, c.bulk_in, c.bulk_out) {
            (Some(problem), _, _) => problem,
            (None, Some(bulk_in), Some(bulk_out)) => {
                if found.interfaces.len() >= MAX_INTERFACES_PER_DEVICE {
                    "too many interfaces"
                } else {
                    found.interfaces.push(MscInterface {
                        configuration: configuration.value,
                        number: c.number,
                        bulk_in,
                        bulk_out,
                        burst_in: c.burst_in,
                        burst_out: c.burst_out,
                    });
                    return;
                }
            }
            _ => "a bulk endpoint is missing",
        };
        found.rejected.push(Rejected {
            number: c.number,
            reason,
        });
    };
    for item in DescriptorIter::new(config) {
        let item = item?;
        match item.descriptor_type {
            libusb::DESCRIPTOR_INTERFACE => {
                finish(current.take(), &mut found);
                let interface = InterfaceDescriptor::parse(item.bytes)?;
                if interface.class != wire::CLASS_MASS_STORAGE || interface.alternate != 0 {
                    continue;
                }
                let problem = if interface.subclass != wire::SUBCLASS_SCSI {
                    Some("not the SCSI transparent command set")
                } else if interface.protocol == wire::PROTOCOL_UAS {
                    Some("UAS without a BOT alternate setting 0")
                } else if interface.protocol != wire::PROTOCOL_BOT {
                    Some("not Bulk-Only Transport")
                } else {
                    None
                };
                current = Some(Candidate {
                    number: interface.number,
                    bulk_in: None,
                    bulk_out: None,
                    burst_in: 0,
                    burst_out: 0,
                    last: None,
                    problem,
                });
            }
            libusb::DESCRIPTOR_ENDPOINT => {
                let Some(c) = current.as_mut() else { continue };
                c.last = None;
                let endpoint = EndpointDescriptor::parse(item.bytes)?;
                if endpoint.transfer_type != TransferType::Bulk {
                    // CBI has an interrupt endpoint; it was already refused.
                    continue;
                }
                if !bulk_packet_size_valid(endpoint.max_packet_size) {
                    c.problem.get_or_insert("invalid bulk packet size");
                    continue;
                }
                let slot = match endpoint.address.direction() {
                    Direction::In => &mut c.bulk_in,
                    Direction::Out => &mut c.bulk_out,
                };
                if slot.is_some() {
                    c.problem
                        .get_or_insert("two bulk endpoints in one direction");
                } else {
                    *slot = Some(endpoint);
                    c.last = Some(endpoint.address.direction());
                }
            }
            libusb::DESCRIPTOR_SS_ENDPOINT_COMPANION => {
                // Follows its endpoint directly.  Streams (bmAttributes) are
                // for UAS; BOT does not use them.
                let Some(c) = current.as_mut() else { continue };
                let Some(direction) = c.last.take() else {
                    continue;
                };
                let burst = *item.bytes.get(2).ok_or(UsbError::InvalidDescriptor)?;
                if burst > MAX_BURST {
                    c.problem.get_or_insert("invalid SuperSpeed max burst");
                    continue;
                }
                match direction {
                    Direction::In => c.burst_in = burst,
                    Direction::Out => c.burst_out = burst,
                }
            }
            _ => {
                if let Some(c) = current.as_mut() {
                    c.last = None;
                }
            }
        }
    }
    finish(current.take(), &mut found);
    Ok(found)
}

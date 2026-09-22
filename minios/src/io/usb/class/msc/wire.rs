//! Bulk-Only Transport wire formats and class requests.
//!
//! USB Mass Storage Class Bulk-Only Transport 1.0: the Command Block Wrapper
//! (section 5.1), the Command Status Wrapper (5.2) and the two class requests
//! (3.1 and 3.2).  The wrappers are little endian; the command block they
//! carry is big endian and is built in [`super::scsi`].

use libusb::{EndpointAddress, SetupPacket};

use super::scsi::Cdb;

pub const CBW_SIGNATURE: u32 = 0x4342_5355;
pub const CSW_SIGNATURE: u32 = 0x5342_5355;
pub const CBW_LEN: usize = 31;
pub const CSW_LEN: usize = 13;

/// Interface class, subclass and protocol of a BOT interface carrying the
/// SCSI transparent command set.
pub const CLASS_MASS_STORAGE: u8 = 0x08;
pub const SUBCLASS_SCSI: u8 = 0x06;
pub const PROTOCOL_BOT: u8 = 0x50;
/// USB Attached SCSI, for the diagnostic that says why an interface was not
/// taken.
pub const PROTOCOL_UAS: u8 = 0x62;

/// A Command Block Wrapper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cbw {
    pub tag: u32,
    pub data_length: u32,
    /// Direction of the data stage.  Meaningless when `data_length` is zero,
    /// and sent as OUT then, as the specification asks.
    pub data_in: bool,
    pub lun: u8,
    pub cdb: Cdb,
}

impl Cbw {
    pub fn to_bytes(&self) -> [u8; CBW_LEN] {
        let mut bytes = [0u8; CBW_LEN];
        bytes[0..4].copy_from_slice(&CBW_SIGNATURE.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.tag.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.data_length.to_le_bytes());
        bytes[12] = if self.data_in && self.data_length != 0 {
            0x80
        } else {
            0
        };
        bytes[13] = self.lun & 0x0f;
        let cdb = self.cdb.as_bytes();
        bytes[14] = cdb.len() as u8;
        bytes[15..15 + cdb.len()].copy_from_slice(cdb);
        bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CswStatus {
    Passed,
    Failed,
    PhaseError,
}

/// A Command Status Wrapper that is valid on its own terms.  Whether it is
/// meaningful — the right tag, a residue within the request — depends on the
/// command it answers and is checked by the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Csw {
    pub tag: u32,
    pub residue: u32,
    pub status: CswStatus,
}

impl Csw {
    /// Section 6.3.1: a CSW is valid when it is exactly 13 bytes and carries
    /// the signature.  A status outside 0..=2 is reserved, and treated as
    /// invalid rather than guessed at.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != CSW_LEN {
            return None;
        }
        let word = |at: usize| {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        if word(0) != CSW_SIGNATURE {
            return None;
        }
        let status = match bytes[12] {
            0 => CswStatus::Passed,
            1 => CswStatus::Failed,
            2 => CswStatus::PhaseError,
            _ => return None,
        };
        Some(Self {
            tag: word(4),
            residue: word(8),
            status,
        })
    }

    pub fn to_bytes(&self) -> [u8; CSW_LEN] {
        let mut bytes = [0u8; CSW_LEN];
        bytes[0..4].copy_from_slice(&CSW_SIGNATURE.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.tag.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.residue.to_le_bytes());
        bytes[12] = match self.status {
            CswStatus::Passed => 0,
            CswStatus::Failed => 1,
            CswStatus::PhaseError => 2,
        };
        bytes
    }
}

/// Get Max LUN (section 3.2): class, interface, IN, one byte.
pub const fn get_max_lun(interface: u8) -> SetupPacket {
    SetupPacket::new(0xa1, 0xfe, 0, interface as u16, 1)
}

/// Bulk-Only Mass Storage Reset (section 3.1): class, interface, OUT, no data.
pub const fn mass_storage_reset(interface: u8) -> SetupPacket {
    SetupPacket::new(0x21, 0xff, 0, interface as u16, 0)
}

/// CLEAR_FEATURE(ENDPOINT_HALT): standard, endpoint, OUT, feature 0.
pub const fn clear_endpoint_halt(endpoint: EndpointAddress) -> SetupPacket {
    SetupPacket::new(0x02, 0x01, 0, endpoint.raw() as u16, 0)
}

#[cfg(test)]
mod tests {
    use super::super::scsi;
    use super::*;

    #[test]
    fn a_cbw_is_little_endian_around_a_big_endian_cdb() {
        let cbw = Cbw {
            tag: 0x1122_3344,
            data_length: 0x0000_4000,
            data_in: true,
            lun: 0,
            cdb: scsi::read_10(0x0102_0304, 0x20),
        };
        let b = cbw.to_bytes();
        assert_eq!(&b[0..4], b"USBC");
        assert_eq!(&b[4..8], &[0x44, 0x33, 0x22, 0x11]);
        assert_eq!(&b[8..12], &[0x00, 0x40, 0x00, 0x00]);
        assert_eq!(b[12], 0x80);
        assert_eq!(b[14], 10);
        assert_eq!(&b[15..25], &[0x28, 0, 1, 2, 3, 4, 0, 0, 0x20, 0]);
        assert!(b[25..].iter().all(|&x| x == 0));
    }

    #[test]
    fn a_cbw_without_data_is_sent_as_out() {
        let cbw = Cbw {
            tag: 1,
            data_length: 0,
            data_in: true,
            lun: 0,
            cdb: scsi::test_unit_ready(),
        };
        assert_eq!(cbw.to_bytes()[12], 0);
    }

    #[test]
    fn a_csw_round_trips_and_rejects_what_is_not_one() {
        let csw = Csw {
            tag: 7,
            residue: 512,
            status: CswStatus::Failed,
        };
        let b = csw.to_bytes();
        assert_eq!(&b[0..4], b"USBS");
        assert_eq!(Csw::parse(&b), Some(csw));
        assert_eq!(Csw::parse(&b[..12]), None, "short");
        let mut long = [0u8; 14];
        long[..13].copy_from_slice(&b);
        assert_eq!(Csw::parse(&long), None, "long");
        let mut bad = b;
        bad[0] = b'X';
        assert_eq!(Csw::parse(&bad), None, "signature");
        let mut bad = b;
        bad[12] = 3;
        assert_eq!(Csw::parse(&bad), None, "reserved status");
    }

    #[test]
    fn class_requests_address_the_interface() {
        assert_eq!(get_max_lun(2).to_bytes(), [0xa1, 0xfe, 0, 0, 2, 0, 1, 0]);
        assert_eq!(
            mass_storage_reset(1).to_bytes(),
            [0x21, 0xff, 0, 0, 1, 0, 0, 0]
        );
        let ep = EndpointAddress::from_raw(0x81).unwrap();
        assert_eq!(
            clear_endpoint_halt(ep).to_bytes(),
            [0x02, 1, 0, 0, 0x81, 0, 0, 0]
        );
    }
}

//! The SCSI commands a read-only block device needs, and their answers.
//!
//! Built against SPC-4 (INCITS 513, r37: 6.4 INQUIRY, 6.39 REQUEST SENSE,
//! 6.47 TEST UNIT READY, 4.5 sense data, 4.5.2 descriptor format, 4.5.3 fixed
//! format) and SBC-3 (INCITS 514, r36: 5.10 READ(10), 5.12 READ(16),
//! 5.15 READ CAPACITY(10), 5.16 READ CAPACITY(16)).  Command blocks are big
//! endian throughout.

/// A command descriptor block of 6, 10 or 16 bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cdb {
    bytes: [u8; 16],
    len: u8,
}

impl Cdb {
    const fn new(len: u8) -> Self {
        Self {
            bytes: [0; 16],
            len,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    pub const fn opcode(&self) -> u8 {
        self.bytes[0]
    }
}

pub mod opcode {
    pub const TEST_UNIT_READY: u8 = 0x00;
    pub const REQUEST_SENSE: u8 = 0x03;
    pub const INQUIRY: u8 = 0x12;
    pub const READ_CAPACITY_10: u8 = 0x25;
    pub const READ_10: u8 = 0x28;
    pub const READ_16: u8 = 0x88;
    pub const SERVICE_ACTION_IN_16: u8 = 0x9e;
    pub const SA_READ_CAPACITY_16: u8 = 0x10;
}

/// Bytes of standard INQUIRY data asked for: enough for the vendor, product
/// and revision fields, which is all a diagnostic listing shows.
pub const INQUIRY_LEN: usize = 36;
/// Fixed-format sense data up to the sense-key specific field.  Descriptor
/// format fits its header in the same room.
pub const SENSE_LEN: usize = 18;
pub const CAPACITY_10_LEN: usize = 8;
pub const CAPACITY_16_LEN: usize = 32;

pub const fn test_unit_ready() -> Cdb {
    Cdb::new(6)
}

pub const fn request_sense(allocation: u8) -> Cdb {
    let mut cdb = Cdb::new(6);
    cdb.bytes[0] = opcode::REQUEST_SENSE;
    cdb.bytes[4] = allocation;
    cdb
}

pub const fn inquiry(allocation: u16) -> Cdb {
    let mut cdb = Cdb::new(6);
    cdb.bytes[0] = opcode::INQUIRY;
    // SPC-4 widened ALLOCATION LENGTH to two bytes; a request below 256 has
    // the same encoding as the one-byte field of older devices.
    let a = allocation.to_be_bytes();
    cdb.bytes[3] = a[0];
    cdb.bytes[4] = a[1];
    cdb
}

pub const fn read_capacity_10() -> Cdb {
    let mut cdb = Cdb::new(10);
    cdb.bytes[0] = opcode::READ_CAPACITY_10;
    cdb
}

pub const fn read_capacity_16(allocation: u32) -> Cdb {
    let mut cdb = Cdb::new(16);
    cdb.bytes[0] = opcode::SERVICE_ACTION_IN_16;
    cdb.bytes[1] = opcode::SA_READ_CAPACITY_16;
    let a = allocation.to_be_bytes();
    cdb.bytes[10] = a[0];
    cdb.bytes[11] = a[1];
    cdb.bytes[12] = a[2];
    cdb.bytes[13] = a[3];
    cdb
}

pub const fn read_10(lba: u32, blocks: u16) -> Cdb {
    let mut cdb = Cdb::new(10);
    cdb.bytes[0] = opcode::READ_10;
    let l = lba.to_be_bytes();
    cdb.bytes[2] = l[0];
    cdb.bytes[3] = l[1];
    cdb.bytes[4] = l[2];
    cdb.bytes[5] = l[3];
    let n = blocks.to_be_bytes();
    cdb.bytes[7] = n[0];
    cdb.bytes[8] = n[1];
    cdb
}

pub const fn read_16(lba: u64, blocks: u32) -> Cdb {
    let mut cdb = Cdb::new(16);
    cdb.bytes[0] = opcode::READ_16;
    let l = lba.to_be_bytes();
    let mut i = 0;
    while i < 8 {
        cdb.bytes[2 + i] = l[i];
        i += 1;
    }
    let n = blocks.to_be_bytes();
    cdb.bytes[10] = n[0];
    cdb.bytes[11] = n[1];
    cdb.bytes[12] = n[2];
    cdb.bytes[13] = n[3];
    cdb
}

/// READ(10) where the range fits it, READ(16) otherwise.  `None` for an empty
/// or unrepresentable range, which is the caller's bug rather than something
/// to send.
pub fn read(lba: u64, blocks: u32) -> Option<Cdb> {
    if blocks == 0 {
        return None;
    }
    let last = lba.checked_add(blocks as u64 - 1)?;
    if last <= u32::MAX as u64 && blocks <= u16::MAX as u32 {
        Some(read_10(lba as u32, blocks as u16))
    } else {
        Some(read_16(lba, blocks))
    }
}

/// What READ CAPACITY reports: the address of the *last* block, not a count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capacity {
    pub last_lba: u64,
    pub block_length: u32,
}

impl Capacity {
    /// READ CAPACITY(10) answers this when the last LBA does not fit, and
    /// READ CAPACITY(16) has to be asked instead.
    pub const LBA_10_OVERFLOW: u64 = 0xffff_ffff;

    pub fn parse_10(bytes: &[u8]) -> Option<Self> {
        let b = bytes.get(..CAPACITY_10_LEN)?;
        Some(Self {
            last_lba: u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as u64,
            block_length: u32::from_be_bytes([b[4], b[5], b[6], b[7]]),
        })
    }

    pub fn parse_16(bytes: &[u8]) -> Option<Self> {
        let b = bytes.get(..12)?;
        let mut lba = [0u8; 8];
        lba.copy_from_slice(&b[..8]);
        Some(Self {
            last_lba: u64::from_be_bytes(lba),
            block_length: u32::from_be_bytes([b[8], b[9], b[10], b[11]]),
        })
    }

    /// The number of blocks, or `None` if it does not fit a u64.
    pub fn block_count(&self) -> Option<u64> {
        self.last_lba.checked_add(1)
    }
}

/// Block lengths this driver takes.  Anything else is refused by name rather
/// than read with the wrong stride.
pub const fn block_length_supported(length: u32) -> bool {
    matches!(length, 512 | 1024 | 2048 | 4096)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Inquiry {
    pub qualifier: u8,
    pub device_type: u8,
    pub removable: bool,
    pub vendor: [u8; 8],
    pub product: [u8; 16],
    pub revision: [u8; 4],
}

impl Inquiry {
    /// Needs the first byte for the device type; the identification strings
    /// are taken as far as the device supplied them and left blank beyond.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let first = *bytes.first()?;
        let mut vendor = [b' '; 8];
        let mut product = [b' '; 16];
        let mut revision = [b' '; 4];
        let copy = |dst: &mut [u8], at: usize| {
            for (i, d) in dst.iter_mut().enumerate() {
                if let Some(&c) = bytes.get(at + i)
                    && (0x20..0x7f).contains(&c)
                {
                    *d = c;
                }
            }
        };
        copy(&mut vendor, 8);
        copy(&mut product, 16);
        copy(&mut revision, 32);
        Some(Self {
            qualifier: first >> 5,
            device_type: first & 0x1f,
            removable: bytes.get(1).is_some_and(|b| b & 0x80 != 0),
            vendor,
            product,
            revision,
        })
    }

    /// A connected direct-access block device (SBC), the one kind this reads.
    pub const fn is_direct_access(&self) -> bool {
        self.qualifier == 0 && self.device_type == 0
    }
}

pub mod sense_key {
    pub const NO_SENSE: u8 = 0x0;
    pub const RECOVERED_ERROR: u8 = 0x1;
    pub const NOT_READY: u8 = 0x2;
    pub const MEDIUM_ERROR: u8 = 0x3;
    pub const HARDWARE_ERROR: u8 = 0x4;
    pub const ILLEGAL_REQUEST: u8 = 0x5;
    pub const UNIT_ATTENTION: u8 = 0x6;
    pub const DATA_PROTECT: u8 = 0x7;
    pub const ABORTED_COMMAND: u8 = 0xb;
}

/// ASC 3Ah: MEDIUM NOT PRESENT, whatever the qualifier.
pub const ASC_MEDIUM_NOT_PRESENT: u8 = 0x3a;
/// ASC 04h: LOGICAL UNIT NOT READY; qualifier 01h is "becoming ready".
pub const ASC_NOT_READY: u8 = 0x04;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Sense {
    pub key: u8,
    pub asc: u8,
    pub ascq: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SenseClass {
    /// No error recorded.  A command that failed with this is treated as a
    /// transient device error.
    NoSense,
    NoMedium,
    /// Not ready for another reason; retried within the readiness window.
    NotReady,
    /// Power-on, reset or medium change: anything learned before is stale.
    UnitAttention,
    MediumError,
    HardwareError,
    IllegalRequest,
    DataProtect,
    Aborted,
    Other,
}

impl Sense {
    /// Parses fixed (70h/71h) or descriptor (72h/73h) format sense data,
    /// using only what the device actually returned.  `None` when there is
    /// not enough for a sense key, or the response code is not one of those.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        match bytes.first()? & 0x7f {
            0x70 | 0x71 => {
                let key = bytes.get(2)? & 0x0f;
                // The ASC and ASCQ are only there if ADDITIONAL SENSE LENGTH
                // says so and the transfer actually carried them.
                let additional = *bytes.get(7).unwrap_or(&0) as usize;
                let (asc, ascq) = if additional >= 6 && bytes.len() >= 14 {
                    (bytes[12], bytes[13])
                } else {
                    (0, 0)
                };
                Some(Self { key, asc, ascq })
            }
            0x72 | 0x73 => Some(Self {
                key: bytes.get(1)? & 0x0f,
                asc: *bytes.get(2).unwrap_or(&0),
                ascq: *bytes.get(3).unwrap_or(&0),
            }),
            _ => None,
        }
    }

    pub const fn class(&self) -> SenseClass {
        match self.key {
            sense_key::NO_SENSE | sense_key::RECOVERED_ERROR => SenseClass::NoSense,
            sense_key::NOT_READY if self.asc == ASC_MEDIUM_NOT_PRESENT => SenseClass::NoMedium,
            sense_key::NOT_READY => SenseClass::NotReady,
            sense_key::UNIT_ATTENTION if self.asc == ASC_MEDIUM_NOT_PRESENT => SenseClass::NoMedium,
            sense_key::UNIT_ATTENTION => SenseClass::UnitAttention,
            sense_key::MEDIUM_ERROR => SenseClass::MediumError,
            sense_key::HARDWARE_ERROR => SenseClass::HardwareError,
            sense_key::ILLEGAL_REQUEST => SenseClass::IllegalRequest,
            sense_key::DATA_PROTECT => SenseClass::DataProtect,
            sense_key::ABORTED_COMMAND => SenseClass::Aborted,
            _ => SenseClass::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_picks_the_ten_byte_form_while_the_range_fits() {
        assert_eq!(read(0, 8).unwrap().opcode(), opcode::READ_10);
        assert_eq!(read(0xffff_fff8, 8).unwrap().opcode(), opcode::READ_10);
        // The last block crosses 2^32.
        assert_eq!(read(0xffff_fff9, 8).unwrap().opcode(), opcode::READ_16);
        assert_eq!(read(0, 0x1_0000).unwrap().opcode(), opcode::READ_16);
        assert_eq!(read(0, 0), None);
        assert_eq!(read(u64::MAX, 2), None);
    }

    #[test]
    fn command_blocks_are_big_endian() {
        assert_eq!(
            read_10(0x0a0b_0c0d, 0x0102).as_bytes(),
            &[0x28, 0, 0x0a, 0x0b, 0x0c, 0x0d, 0, 0x01, 0x02, 0]
        );
        let r16 = read_16(0x0102_0304_0506_0708, 0x0a0b_0c0d);
        assert_eq!(
            r16.as_bytes(),
            &[
                0x88, 0, 1, 2, 3, 4, 5, 6, 7, 8, 0x0a, 0x0b, 0x0c, 0x0d, 0, 0
            ]
        );
        let rc16 = read_capacity_16(32);
        assert_eq!(rc16.as_bytes()[..2], [0x9e, 0x10]);
        assert_eq!(rc16.as_bytes()[10..14], [0, 0, 0, 32]);
        assert_eq!(inquiry(36).as_bytes(), &[0x12, 0, 0, 0, 36, 0]);
        assert_eq!(request_sense(18).as_bytes(), &[0x03, 0, 0, 0, 18, 0]);
        assert_eq!(test_unit_ready().as_bytes(), &[0; 6]);
    }

    #[test]
    fn capacity_is_the_last_block_and_the_count_is_checked() {
        let c = Capacity::parse_10(&[0, 0, 0x3f, 0xff, 0, 0, 2, 0]).unwrap();
        assert_eq!(c.last_lba, 0x3fff);
        assert_eq!(c.block_length, 512);
        assert_eq!(c.block_count(), Some(0x4000));
        assert_eq!(Capacity::parse_10(&[0; 7]), None);
        let mut b = [0u8; 32];
        b[..8].copy_from_slice(&u64::MAX.to_be_bytes());
        b[8..12].copy_from_slice(&4096u32.to_be_bytes());
        let c = Capacity::parse_16(&b).unwrap();
        assert_eq!(c.block_length, 4096);
        assert_eq!(
            c.block_count(),
            None,
            "a count past u64 is refused, not wrapped"
        );
    }

    #[test]
    fn only_the_listed_block_lengths_are_taken() {
        for ok in [512, 1024, 2048, 4096] {
            assert!(block_length_supported(ok));
        }
        for bad in [0, 256, 520, 8192] {
            assert!(!block_length_supported(bad));
        }
    }

    #[test]
    fn inquiry_reads_the_type_and_the_strings_it_was_given() {
        let mut b = [0u8; 36];
        b[0] = 0x00;
        b[1] = 0x80;
        b[8..16].copy_from_slice(b"QEMU    ");
        b[16..32].copy_from_slice(b"QEMU HARDDISK   ");
        let i = Inquiry::parse(&b).unwrap();
        assert!(i.is_direct_access());
        assert!(i.removable);
        assert_eq!(&i.vendor, b"QEMU    ");
        assert_eq!(&i.product, b"QEMU HARDDISK   ");
        let cdrom = Inquiry::parse(&[0x05]).unwrap();
        assert!(!cdrom.is_direct_access());
        assert_eq!(&cdrom.vendor, b"        ");
        let absent = Inquiry::parse(&[0x7f]).unwrap();
        assert!(!absent.is_direct_access(), "qualifier 3: no unit there");
        assert_eq!(Inquiry::parse(&[]), None);
    }

    #[test]
    fn fixed_sense_needs_its_additional_length_for_the_asc() {
        let mut b = [0u8; 18];
        b[0] = 0x70;
        b[2] = sense_key::NOT_READY;
        b[7] = 10;
        b[12] = ASC_MEDIUM_NOT_PRESENT;
        let s = Sense::parse(&b).unwrap();
        assert_eq!(s.class(), SenseClass::NoMedium);
        // The same bytes cut short of the ASC say only "not ready".
        let s = Sense::parse(&b[..8]).unwrap();
        assert_eq!(s.asc, 0);
        assert_eq!(s.class(), SenseClass::NotReady);
        b[7] = 0;
        assert_eq!(Sense::parse(&b).unwrap().asc, 0, "additional length 0");
    }

    #[test]
    fn descriptor_sense_and_unknown_formats() {
        let s = Sense::parse(&[0x72, sense_key::UNIT_ATTENTION, 0x28, 0x00]).unwrap();
        assert_eq!(s.class(), SenseClass::UnitAttention);
        assert_eq!(Sense::parse(&[0x72]), None);
        assert_eq!(Sense::parse(&[0x70, 0]), None);
        assert_eq!(Sense::parse(&[0x00; 18]), None);
        assert_eq!(Sense::parse(&[]), None);
        let s = Sense {
            key: 0xe,
            asc: 0,
            ascq: 0,
        };
        assert_eq!(s.class(), SenseClass::Other);
    }
}

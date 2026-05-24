use core::mem::{size_of, transmute};

/// Generic Device Path Node
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericDevicePathNode {
    /// The type of the device path node.
    type_: u8,
    /// The subtype of the device path node.
    sub_type: u8,
    /// The length of the device path node, including the header.
    length: DpWord,
}

impl GenericDevicePathNode {
    /// The end of device path node.
    pub const END: Self = Self {
        type_: 0x7f,
        sub_type: 0xff,
        length: DpWord([4, 0]),
    };

    pub fn from_bytes<'a>(bytes: &'a [u8]) -> Option<&'a Self> {
        if bytes.len() < size_of::<Self>() {
            return None;
        }
        // SAFETY: size is checked above
        unsafe { Some(transmute(bytes.as_ptr())) }
    }

    pub const fn type_(&self) -> Option<(Type, u8)> {
        match Type::from_u8(self.type_) {
            Some(t) => Some((t, self.sub_type)),
            None => None,
        }
    }

    #[inline]
    pub const fn len(&self) -> u16 {
        self.length.get()
    }

    #[inline]
    pub fn is_end(&self) -> bool {
        *self == Self::END
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpWord(pub [u8; 2]);

impl DpWord {
    #[inline]
    pub const fn get(&self) -> u16 {
        u16::from_le_bytes(self.0)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpDword(pub [u8; 4]);

impl DpDword {
    #[inline]
    pub const fn get(&self) -> u32 {
        u32::from_le_bytes(self.0)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpQword(pub [u8; 8]);

impl DpQword {
    #[inline]
    pub const fn get(&self) -> u64 {
        u64::from_le_bytes(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Type {
    /// Hardware Device Path
    Hardware = 0x01,
    /// ACPI Device Path
    Acpi = 0x02,
    /// Messaging Device Path
    Messaging = 0x03,
    /// Media Device Path
    Media = 0x04,
    /// BIOS Boot Specification Device Path
    Bbs = 0x05,
    /// End of Hardware Device Path
    End = 0x7f,
}

impl Type {
    #[inline]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::Hardware),
            0x02 => Some(Self::Acpi),
            0x03 => Some(Self::Messaging),
            0x04 => Some(Self::Media),
            0x05 => Some(Self::Bbs),
            0x7f => Some(Self::End),
            _ => None,
        }
    }
}

#[repr(C)]
pub struct HardDriveMedia {
    header: GenericDevicePathNode,
    partition_number: DpDword,
    partition_start: DpQword,
    partition_size: DpQword,
    partition_signature: [u8; 16],
    partition_format: u8,
    signature_type: u8,
}

impl HardDriveMedia {
    pub fn parse<'a>(bytes: &'a [u8]) -> Option<&'a Self> {
        if bytes.len() < size_of::<Self>() {
            return None;
        }
        // SAFETY: size is checked above
        let result: &Self = unsafe { transmute(bytes.as_ptr()) };

        result.header.type_().and_then(|(t, sub)| {
            if t == Type::Media && sub == 0x01 {
                Some(result)
            } else {
                None
            }
        })
    }

    #[inline]
    pub const fn partition_number(&self) -> u32 {
        self.partition_number.get()
    }

    #[inline]
    pub const fn partition_start(&self) -> u64 {
        self.partition_start.get()
    }

    #[inline]
    pub const fn partition_size(&self) -> u64 {
        self.partition_size.get()
    }

    #[inline]
    pub const fn partition_format(&self) -> PartitionFormat {
        PartitionFormat::from_u8(self.partition_format)
    }

    pub fn partition_signature(&self) -> PartitionSignature {
        match SignatureType::from_u8(self.signature_type) {
            SignatureType::None => PartitionSignature::Unknown,
            SignatureType::Mbr => {
                let mut sig = [0u8; 4];
                sig.copy_from_slice(&self.partition_signature[0..4]);
                PartitionSignature::Mbr(sig)
            }
            SignatureType::Gpt => {
                let mut sig = [0u8; 16];
                sig.copy_from_slice(&self.partition_signature[0..16]);
                PartitionSignature::Gpt(sig)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionSignature {
    Unknown,
    Mbr([u8; 4]),
    Gpt([u8; 16]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionFormat {
    Unknown,
    Mbr,
    Gpt,
}

impl PartitionFormat {
    #[inline]
    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Mbr,
            2 => Self::Gpt,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureType {
    None,
    Mbr,
    Gpt,
}

impl SignatureType {
    #[inline]
    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Mbr,
            2 => Self::Gpt,
            _ => Self::None,
        }
    }
}

#[repr(C)]
pub struct CdromMedia {
    header: GenericDevicePathNode,
    boot_entry_id: DpWord,
    partition_start: DpQword,
    partition_size: DpQword,
}

impl CdromMedia {
    pub fn parse<'a>(bytes: &'a [u8]) -> Option<&'a Self> {
        if bytes.len() < size_of::<Self>() {
            return None;
        }
        // SAFETY: size is checked above
        let result: &Self = unsafe { transmute(bytes.as_ptr()) };

        result.header.type_().and_then(|(t, sub)| {
            if t == Type::Media && sub == 0x02 {
                Some(result)
            } else {
                None
            }
        })
    }
}

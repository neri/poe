/// Directory entry in FAT filesystem
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DosDirEnt {
    /// 11 bytes: 8 for name, 3 for extension
    pub name: [u8; 11],
    /// File attributes
    pub attr: DosAttributes,
    /// Reserved for Windows NT
    pub nt_reserved: u8,
    /// Creation time in milliseconds (0-199)
    pub ctime_ms: u8,
    /// Creation time
    pub ctime: DosFileTimeStamp,
    /// Last access date
    pub atime: DosFileDate,
    /// High word of first cluster (FAT32 only)
    pub cluster_hi: u16,
    /// Last modification time
    pub mtime: DosFileTimeStamp,
    /// Low word of first cluster
    pub first_cluster: u16,
    /// File size in bytes
    pub file_size: u32,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DosAttributes(u8);

impl DosAttributes {
    /// File is read-only
    pub const READONLY: Self = Self(0b0000_0001);
    /// File is hidden
    pub const HIDDEN: Self = Self(0b0000_0010);
    /// File is a system file
    pub const SYSTEM: Self = Self(0b0000_0100);
    /// Volume label
    pub const LABEL: Self = Self(0b0000_1000);
    /// Directory entry is a subdirectory
    pub const SUBDIR: Self = Self(0b0001_0000);
    /// Archive
    pub const ARCHIVE: Self = Self(0b0010_0000);
    /// Long File Name entry
    pub const LFN_ENTRY: Self = Self(0b0000_1111);

    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DosDirEntType {
    File,
    Directory,
    VolumeLabel,
    LfnEntry,
}

impl DosDirEntType {
    #[inline]
    pub fn from_attr(attr: DosAttributes) -> Self {
        if attr == DosAttributes::LFN_ENTRY {
            Self::LfnEntry
        } else if (attr.0 & DosAttributes::LABEL.0) != 0 {
            Self::VolumeLabel
        } else if (attr.0 & DosAttributes::SUBDIR.0) != 0 {
            Self::Directory
        } else {
            Self::File
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DosFileTime(pub u16);

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DosFileDate(pub u16);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DosFileTimeStamp {
    pub time: DosFileTime,
    pub date: DosFileDate,
}

impl DosFileTime {
    pub const EMPTY: Self = Self(0);
}

impl DosFileDate {
    pub const EMPTY: Self = Self(0);
}

impl DosFileTimeStamp {
    pub const EMPTY: Self = Self {
        time: DosFileTime::EMPTY,
        date: DosFileDate::EMPTY,
    };
}

impl DosDirEnt {
    #[inline]
    pub const fn new() -> Self {
        Self {
            name: [0x20; 11],
            attr: DosAttributes::empty(),
            nt_reserved: 0,
            ctime_ms: 0,
            ctime: DosFileTimeStamp::EMPTY,
            atime: DosFileDate::EMPTY,
            mtime: DosFileTimeStamp::EMPTY,
            first_cluster: 0,
            cluster_hi: 0,
            file_size: 0,
        }
    }

    #[inline]
    pub fn entry_type(&self) -> DosDirEntType {
        DosDirEntType::from_attr(self.attr)
    }

    pub fn volume_label(label: &str) -> Result<Self, ConvertError> {
        let mut result = Self::new();
        result.attr = DosAttributes::LABEL;

        let mut label = label.chars();
        for i in 0..11 {
            let c = match label.next() {
                Some(c) => c,
                None => break,
            };
            let c = match Self::validate_volname_char(c) {
                Some(c) => c,
                None => return Err(ConvertError::InvalidChar),
            };
            result.name[i] = c;
        }
        Ok(result)
    }

    pub fn file_entry(name: &str) -> Result<Self, ConvertError> {
        let mut result = Self::new();
        result.attr = DosAttributes::ARCHIVE;

        let mut has_ext = true;
        let mut has_to_truncate = true;
        let mut name_has_upper = false;
        let mut name_has_lower = false;
        let mut ext_has_upper = false;
        let mut ext_has_lower = false;
        let mut chars = name.chars();

        for i in 0..8 {
            let c = match chars.next() {
                Some('.') => {
                    has_to_truncate = false;
                    break;
                }
                Some(c) => c,
                None => {
                    has_ext = false;
                    break;
                }
            };
            name_has_upper |= c.is_uppercase();
            name_has_lower |= c.is_lowercase();

            if let Some(c) = Self::validate_shortname_char(c) {
                result.name[i] = c;
            } else {
                return Err(ConvertError::InvalidChar);
            }
        }

        if has_to_truncate {
            loop {
                match chars.next() {
                    None => {
                        has_ext = false;
                        break;
                    }
                    Some('.') => break,
                    _ => (),
                }
            }
        }

        if has_ext {
            for i in 8..11 {
                let c = match chars.next() {
                    Some(c) => c,
                    None => break,
                };

                ext_has_upper |= c.is_uppercase();
                ext_has_lower |= c.is_lowercase();

                if let Some(c) = Self::validate_shortname_char(c) {
                    result.name[i] = c;
                } else {
                    return Err(ConvertError::InvalidChar);
                }
            }
        }

        result.nt_reserved = if name_has_lower & !name_has_upper {
            0x08
        } else {
            0
        } | if ext_has_lower & !ext_has_upper {
            0x10
        } else {
            0
        };

        if result.name[0] != 0x20 {
            Ok(result)
        } else {
            Err(ConvertError::Empty)
        }
    }

    /// Computes the checksum for a short name, used in LFN entries.
    pub fn lfn_checksum(&self) -> u8 {
        let mut sum = 0u8;
        for &c in self.name.iter() {
            sum = ((sum & 1) << 7) + (sum >> 1) + c;
        }
        sum
    }

    fn validate_volname_char(c: char) -> Option<u8> {
        let c = c as u8;
        match c {
            0x20
            | 0x21
            | 0x23..=0x29
            | 0x2D
            | 0x30..=0x39
            | 0x41..=0x5A
            | 0x5E
            | 0x5F
            | 0x7B
            | 0x7D
            | 0x7E => Some(c),
            0x61..=0x7A => Some(c - 0x20),
            _ => None,
        }
    }

    fn validate_shortname_char(c: char) -> Option<u8> {
        let c = c as u8;
        match c {
            0x21
            | 0x23..=0x29
            | 0x2D
            | 0x30..=0x39
            | 0x41..=0x5A
            | 0x5E
            | 0x5F
            | 0x7B
            | 0x7D
            | 0x7E => Some(c),
            0x61..=0x7A => Some(c - 0x20),
            _ => None,
        }
    }
}

impl Default for DosDirEnt {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertError {
    Empty,
    InvalidChar,
}

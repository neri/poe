// System Management BIOS
#![cfg_attr(not(test), no_std)]

extern crate alloc;
use core::{ffi::c_void, marker::PhantomData, slice, str};

/// EFI GUID of the SMBIOS 1.0 table.
#[cfg(feature = "guid")]
pub const SMBIOS_GUID: guid::Guid = guid::guid!("eb9d2d31-2d88-11d3-9a16-0090273fc14d");

/// EFI GUID of the SMBIOS 3.0 table.
#[cfg(feature = "guid")]
pub const SMBIOS3_GUID: guid::Guid = guid::guid!("f2fd1544-9794-4a2c-992e-e5bbcf20e394");

/// System Management BIOS Entry Point
pub struct SmBios {
    base: *const u8,
    n_structures: usize,
    ver_major: u8,
    ver_minor: u8,
    table_length: u16,
}

impl SmBios {
    #[inline]
    pub unsafe fn parse(ptr: *const c_void) -> Option<Self> {
        let ep = unsafe { &*(ptr as *const SmBiosEntryV1) };
        ep.is_valid().then(|| ())?;

        Some(Self {
            base: ep.base as *const u8,
            n_structures: ep.n_structures as usize,
            ver_major: ep.ver_major,
            ver_minor: ep.ver_minor,
            table_length: ep.structure_table_len,
        })
    }

    #[inline]
    pub const fn major_version(&self) -> u8 {
        self.ver_major
    }

    #[inline]
    pub const fn minor_version(&self) -> u8 {
        self.ver_minor
    }

    #[inline]
    pub const fn n_structures(&self) -> usize {
        self.n_structures
    }

    #[inline]
    pub const fn table_length(&self) -> usize {
        self.table_length as usize
    }

    /// Returns the system manufacturer name, if available
    #[inline]
    pub fn manufacturer(&self) -> Option<&str> {
        self.find(HeaderType::SYSTEM_INFO).and_then(|h| {
            let slice = h.as_slice();
            h.string(slice[4] as usize)
        })
    }

    /// Returns the system product name, if available
    #[inline]
    pub fn product_name(&self) -> Option<&str> {
        self.find(HeaderType::SYSTEM_INFO).and_then(|h| {
            let slice = h.as_slice();
            h.string(slice[5] as usize)
        })
    }

    /// Returns the serial number, if available
    #[inline]
    pub fn serial_number(&self) -> Option<&str> {
        self.find(HeaderType::SYSTEM_INFO).and_then(|h| {
            let slice = h.as_slice();
            h.string(slice[7] as usize)
        })
    }

    #[inline]
    pub fn system_uuid(&self) -> Option<uuid::Uuid> {
        self.find(HeaderType::SYSTEM_INFO).and_then(|h| {
            let slice = h.as_slice();
            if slice.len() >= 0x19 {
                let raw = &slice[0x08..0x18];
                let uuid = uuid::Uuid::from_bytes(raw.try_into().unwrap());
                if uuid != uuid::Uuid::null() {
                    Some(uuid)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    /// Returns an iterator that iterates through the SMBIOS structure
    #[inline]
    pub fn iter<'a>(&self) -> impl Iterator<Item = &'a SmBiosHeader> {
        SmBiosStructIterator {
            base: self.base,
            offset: 0,
            index: 0,
            limit: self.n_structures,
            _phantom: PhantomData,
        }
    }

    /// Find the first structure matching the specified header type.
    #[inline]
    pub fn find<'a>(&self, header_type: HeaderType) -> Option<&'a SmBiosHeader> {
        self.iter().find(|v| v.header_type() == header_type)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HeaderType(pub u8);

impl HeaderType {
    pub const BIOS_INFO: Self = Self(0);
    pub const SYSTEM_INFO: Self = Self(1);
    pub const BASEBOARD_INFO: Self = Self(2);
    pub const SYSTEM_ENCLOSURE: Self = Self(3);
    pub const PROCESSOR_INFO: Self = Self(4);
    pub const MEMORY_CONTROLLER_INFO: Self = Self(5);
    pub const MEMORY_MODULE_INFO: Self = Self(6);
    pub const CACHE_INFO: Self = Self(7);
    pub const PORT_CONNECTOR_INFO: Self = Self(8);
    pub const SYSTEM_SLOTS: Self = Self(9);
    pub const ONBOARD_DEVICE_INFO: Self = Self(10);
    pub const OEM_STRINGS: Self = Self(11);
    pub const SYSTEM_CONFIGURATION_OPTIONS: Self = Self(12);
    pub const BIOS_LANGUAGE_INFO: Self = Self(13);
    pub const GROUP_ASSOCIATIONS: Self = Self(14);
    pub const SYSTEM_EVENT_LOG: Self = Self(15);
    pub const PHYSICAL_MEMORY_ARRAY: Self = Self(16);
    pub const MEMORY_DEVICE: Self = Self(17);
    pub const _32BIT_MEMORY_ERROR_INFO: Self = Self(18);
    pub const MEMORY_ARRAY_MAPPED_ADDRESS: Self = Self(19);
    pub const MEMORY_DEVICE_MAPPED_ADDRESS: Self = Self(20);
    pub const BUILT_IN_POINTING_DEVICE: Self = Self(21);
    pub const PORTABLE_BATTERY: Self = Self(22);
    pub const SYSTEM_RESET: Self = Self(23);
    pub const HARDWARE_SECURITY: Self = Self(24);
    pub const SYSTEM_POWER_CONTROLS: Self = Self(25);
    pub const VOLTAGE_PROBE: Self = Self(26);
    pub const COOLING_DEVICE: Self = Self(27);
    pub const TEMPERATURE_PROBE: Self = Self(28);
    pub const ELECTRICAL_CURRENT_PROBE: Self = Self(29);
    pub const OUT_OF_BAND_REMOTE_ACCESS: Self = Self(30);
    pub const BOOT_INTEGRITY_SERVICE: Self = Self(31);
    pub const SYSTEM_BOOT_INFO: Self = Self(32);
    pub const _64BIT_MEMORY_ERROR_INFO: Self = Self(33);
    pub const MANAGEMENT_DEVICE: Self = Self(34);
    pub const MANAGEMENT_DEVICE_COMPONENT: Self = Self(35);
    pub const MANAGEMENT_DEVICE_THRESHOLD_DATA: Self = Self(36);
    pub const MEMORY_CHANNEL: Self = Self(37);
    pub const IPMI_DEVICE_INFO: Self = Self(38);
    pub const SYSTEM_POWER_SUPPLY: Self = Self(39);
    pub const ADDITIONAL_INFO: Self = Self(40);
    pub const ONBOARD_DEVICES_EXTENDED_INFO: Self = Self(41);
    pub const MANAGEMENT_CONTROLLER_HOST_INTERFACE: Self = Self(42);
    pub const TPM_DEVICE: Self = Self(43);
    pub const PROCESSOR_ADDITIONAL_INFO: Self = Self(44);
}

#[repr(C)]
#[allow(dead_code)]
pub struct SmBiosEntryV1 {
    /// Anchor string "_SM_"
    anchor: [u8; 4],
    /// Checksum of the Entry Point Structure (EPS)
    /// This value, when added to all other bytes in the EPS,
    /// results in the value 00h (using 8-bit addition calculations).
    /// Values in the EPS are summed starting at offset 00h, for Entry Point Length bytes.
    checksum: u8,
    /// Length of the entry point structure, typically 0x1F
    len: u8,
    /// SMBIOS major version
    ver_major: u8,
    /// SMBIOS minor version
    ver_minor: u8,
    /// Maximum structure size
    max_struct: u16,
    /// Entry point revision
    revision: u8,
    /// Formatted Area
    /// Value present in the Entry Point Revision field defines the interpretation to be placed upon these 5 bytes
    formatted: [u8; 5],
    /// Anchor string "_DMI_"
    anchor2: [u8; 5],
    /// Checksum of Intermediate Entry Point Structure (IEPS).
    /// This value, when added to all other bytes in the IEPS,
    /// results in the value 00h (using 8-bit addition calculations).
    /// Values in the IEPS are summed starting at offset 10h, for 0Fh bytes.
    checksum2: u8,
    /// Length of the structure table
    structure_table_len: u16,
    /// Physical address of the SMBIOS structure table
    base: u32,
    /// Number of SMBIOS structures
    n_structures: u16,
    /// SMBIOS BCD revision
    rev: u8,
}

impl SmBiosEntryV1 {
    pub const ANCHOR: [u8; 4] = *b"_SM_";
    pub const ANCHOR2: [u8; 5] = *b"_DMI_";

    #[inline]
    pub fn is_valid_anchor(&self) -> bool {
        (self.anchor == Self::ANCHOR) && (self.anchor2 == Self::ANCHOR2)
    }

    pub fn is_valid(&self) -> bool {
        if self.is_valid_anchor() == false {
            return false;
        }

        let base = self as *const _ as *const u8;
        let sum1: u8 = unsafe {
            let slice = slice::from_raw_parts(base, self.len as usize);
            slice.iter().copied().sum()
        };
        let sum2: u8 = unsafe {
            let slice = slice::from_raw_parts(base.add(0x10), 0x0F);
            slice.iter().copied().sum()
        };
        (sum1 == 0) && (sum2 == 0)
    }
}

struct SmBiosStructIterator<'a> {
    base: *const u8,
    offset: usize,
    index: usize,
    limit: usize,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> Iterator for SmBiosStructIterator<'a> {
    type Item = &'a SmBiosHeader;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.limit {
            return None;
        }
        unsafe {
            let p = self.base.add(self.offset) as *const SmBiosHeader;
            let r = &*p;
            self.offset += r.struct_size();
            self.index += 1;
            Some(r)
        }
    }
}

/// Common definition of SmBios's structures
#[repr(C)]
pub struct SmBiosHeader {
    header_type: HeaderType,
    size: u8,
    handle: u16,
}

impl SmBiosHeader {
    /// Some products return meaningless strings.
    pub const DEFAULT_STRING: &str = "Default string";
    /// Some products return meaningless strings.
    pub const TO_BE_FILLED_BY_OEM: &str = "To be filled by O.E.M.";

    #[inline]
    pub const fn header_type(&self) -> HeaderType {
        self.header_type
    }

    #[inline]
    pub const fn header_size(&self) -> usize {
        self.size as usize
    }

    #[inline]
    pub fn handle(&self) -> Handle {
        Handle(unsafe { (&self.handle as *const u16).read_unaligned() })
    }

    #[inline]
    pub fn as_slice<'a>(&'a self) -> &'a [u8] {
        let data = self as *const _ as *const u8;
        let len = self.header_size();
        unsafe { slice::from_raw_parts(data, len) }
    }

    #[inline]
    fn strings<'a>(&self) -> SmBiosStrings<'a> {
        let base = unsafe { (self as *const _ as *const u8).add(self.header_size()) };
        SmBiosStrings {
            base,
            offset: 0,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn string<'a>(&'a self, index: usize) -> Option<&'a str> {
        if index > 0 {
            self.strings().nth(index - 1).and_then(|v| match v {
                Self::DEFAULT_STRING | Self::TO_BE_FILLED_BY_OEM => None,
                _ => Some(v),
            })
        } else {
            None
        }
    }

    #[inline]
    pub fn struct_size(&self) -> usize {
        let mut iter = self.strings();
        while iter.next().is_some() {}
        if iter.offset > 0 {
            // There is a NULL after some strings
            self.header_size() + iter.offset + 1
        } else {
            // There is no strings and a double NULL
            self.header_size() + 2
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle(pub u16);

struct SmBiosStrings<'a> {
    base: *const u8,
    offset: usize,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> Iterator for SmBiosStrings<'a> {
    type Item = &'a str;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let ptr = self.base.add(self.offset);
            let len = strlen(ptr);
            if len > 0 {
                self.offset += len + 1;
                Some(str::from_utf8(slice::from_raw_parts(ptr, len)).unwrap_or("?"))
            } else {
                None
            }
        }
    }
}

#[inline]
unsafe fn strlen(p: *const u8) -> usize {
    let mut count = 0;
    loop {
        if unsafe { p.add(count).read_volatile() } == 0 {
            break count;
        } else {
            count += 1;
        }
    }
}

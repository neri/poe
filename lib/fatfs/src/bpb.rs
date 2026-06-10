//! BIOS Parameter Block (BPB) and related structures for FAT filesystem.

use alloc::boxed::Box;
use core::mem::transmute;

use crate::FatType;

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
pub struct Bpb {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors_count: u16,
    pub n_fats: u8,
    pub root_entries_count: u16,
    pub total_sectors: u16,
    pub media_descriptor: u8,
    pub sectors_per_fat: u16,
    pub sectors_per_track: u16,
    pub n_heads: u16,
    pub hidden_sectors_count: u32,
    pub total_sectors32: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct ExtendedBpb {
    pub bpb: Bpb,
    pub physical_drive_number: u8,
    pub reserved: u8,
    pub extended_boot_sign: u8,
    pub volume_serial_number: u32,
    pub volume_label: [u8; 11],
    pub filesystem: [u8; 8],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct ExtendedBpb32 {
    pub bpb: Bpb,
    pub sectors_per_fat32: u32,
    pub flags: u16,
    pub version: u16,
    pub root_cluster: u32,
    pub fs_info_sector: u16,
    pub backup_boot_sector: u16,
    pub reserved29: [u8; 12],
    pub physical_drive_number: u8,
    pub reserved36: u8,
    pub extended_boot_sign: u8,
    pub volume_serial_number: u32,
    pub volume_label: [u8; 11],
    pub filesystem: [u8; 8],
}

impl Bpb {
    #[inline]
    pub const fn new(
        bytes_per_sector: u16,
        sectors_per_cluster: u8,
        reserved_sectors_count: u16,
        n_fats: u8,
        root_entries_count: u16,
        media_descriptor: u8,
        sectors_per_fat: u16,
        sectors_per_track: u16,
        n_heads: u16,
        n_cylinders: u16,
    ) -> Self {
        let total_sectors = n_cylinders * n_heads * sectors_per_track;
        Self {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors_count,
            n_fats,
            root_entries_count,
            total_sectors,
            media_descriptor,
            sectors_per_fat,
            sectors_per_track,
            n_heads,
            hidden_sectors_count: 0,
            total_sectors32: 0,
        }
    }

    // pub fn parse_type(opt: &str) -> Option<Self> {
    //     match opt {
    //         "2hd" | "1440" => Some(Self::new(512, 1, 1, 2, 224, 0xF0, 9, 18, 2, 80)),
    //         "2hc" | "1200" => Some(Self::new(512, 1, 1, 2, 224, 0xF9, 7, 15, 2, 80)),
    //         "nec" | "1232" => Some(Self::new(1024, 1, 1, 2, 192, 0xFE, 2, 8, 2, 77)),
    //         "2dd" | "720" => Some(Self::new(512, 2, 1, 2, 112, 0xF9, 3, 9, 2, 80)),
    //         "640" => Some(Self::new(512, 2, 1, 2, 112, 0xFB, 2, 8, 2, 80)),
    //         "320" => Some(Self::new(512, 2, 1, 2, 112, 0xFF, 2, 8, 2, 40)),
    //         "160" => Some(Self::new(512, 1, 1, 2, 64, 0xFE, 1, 8, 1, 40)),
    //         _ => None,
    //     }
    // }

    pub fn total_sectors(&self) -> Option<u32> {
        if self.total_sectors != 0 {
            Some(self.total_sectors as u32)
        } else if self.total_sectors32 != 0 {
            Some(self.total_sectors32)
        } else {
            None
        }
    }
}

impl ExtendedBpb {
    pub const EXTENDED_BOOT_SIGN: u8 = 0x29;

    /// Returns `true` if the Extended BPB has a valid signature.
    #[inline]
    pub const fn is_valid(&self) -> bool {
        self.extended_boot_sign == Self::EXTENDED_BOOT_SIGN
    }
}

impl Default for ExtendedBpb {
    #[inline]
    fn default() -> Self {
        Self {
            bpb: Bpb::default(),
            physical_drive_number: 0,
            reserved: 0,
            extended_boot_sign: Self::EXTENDED_BOOT_SIGN,
            volume_serial_number: 0,
            volume_label: *b"NO NAME    ",
            filesystem: *b"FAT12   ",
        }
    }
}

impl ExtendedBpb32 {
    /// Returns `true` if the Extended BPB has a valid signature.
    #[inline]
    pub const fn is_valid(&self) -> bool {
        self.extended_boot_sign == ExtendedBpb::EXTENDED_BOOT_SIGN
    }
}

#[repr(C, packed)]
pub struct BootSector {
    jumps: [u8; 3],
    oem_name: [u8; 8],
    ebpb: ExtendedBpb,
    boot_code: [u8; 0x1C0],
    boot_signature: [u8; 2],
}

impl BootSector {
    pub const PREFERRED_SIZE: usize = 512;

    pub const BOOT_SIGNATURE: [u8; 2] = [0x55, 0xAA];

    #[inline]
    pub fn as_bytes(&self) -> &[u8; Self::PREFERRED_SIZE] {
        unsafe { transmute(self) }
    }

    #[inline]
    pub fn into_boxed_bytes(self) -> Box<[u8; Self::PREFERRED_SIZE]> {
        Box::new(unsafe { transmute(self) })
    }

    /// Identifies the FAT type based on the BPB fields.
    pub fn fat_type(&self) -> Option<FatType> {
        let bpb = &self.ebpb.bpb;
        let total_sectors = bpb.total_sectors()?;

        let (sectors_per_fat, maybe_fat32) = if bpb.sectors_per_fat > 0 {
            (bpb.sectors_per_fat as u32, false)
        } else {
            let ebpb32 = unsafe { &*(bpb as *const _ as *const ExtendedBpb32) };
            if ebpb32.is_valid() && ebpb32.sectors_per_fat32 > 0 {
                (ebpb32.sectors_per_fat32, true)
            } else {
                return None;
            }
        };

        let sector_size = bpb.bytes_per_sector as u32;
        let offset_fat = bpb.reserved_sectors_count as u32;
        let offset_root = offset_fat + (bpb.n_fats as u32 * sectors_per_fat);
        let offset_cluster =
            offset_root + (bpb.root_entries_count as u32 * 32 + sector_size - 1) / sector_size;
        let total_records = (total_sectors - offset_cluster) / bpb.sectors_per_cluster as u32;

        // Finally, determine the FAT type based on the total number of clusters.
        if total_records < 0xff5 {
            (!maybe_fat32).then(|| FatType::Fat12)
        } else if total_records < 0xfff5 {
            (!maybe_fat32).then(|| FatType::Fat16)
        } else if total_records < 0xff_ffff5 {
            maybe_fat32.then(|| FatType::Fat32)
        } else {
            None
        }
    }

    /// Identifies the FAT type based on the given boot sector bytes.
    pub fn identify(bytes: &[u8]) -> Option<FatType> {
        if bytes.len() < Self::PREFERRED_SIZE {
            return None;
        }
        let boot_sector = unsafe { &*(bytes.as_ptr() as *const Self) };
        let bpb = &boot_sector.ebpb.bpb;

        // The BPB must satisfy the following conditions to be considered valid:
        if !bpb.bytes_per_sector.is_power_of_two()
            || !bpb.sectors_per_cluster.is_power_of_two()
            || bpb.reserved_sectors_count == 0
            || ![1, 2].contains(&bpb.n_fats)
            || bpb.sectors_per_track == 0
            || bpb.n_heads == 0
        {
            return None;
        }

        let fat_type = boot_sector.fat_type()?;

        // Additional checks for FAT12/16 and FAT32.
        match fat_type {
            FatType::Fat12 | FatType::Fat16 => {
                //
            }
            FatType::Fat32 => {
                let ebpb32 = unsafe { &*(bytes.as_ptr().byte_add(0x0b) as *const ExtendedBpb32) };
                // Note: The third byte of `sectors_per_fat32` should never become `0x28` or `0x29`
                if ebpb32.sectors_per_fat32 > 0x200000
                    || ebpb32.root_cluster == 0
                    || ebpb32.fs_info_sector == 0
                {
                    return None;
                }
            }
        }

        Some(fat_type)
    }

    #[inline]
    pub fn from_bytes<'a>(bytes: &'a [u8]) -> Option<&'a Self> {
        Self::identify(bytes).map(|_| unsafe { &*(bytes.as_ptr() as *const Self) })
    }

    #[inline]
    pub fn bpb(&self) -> &Bpb {
        &self.ebpb.bpb
    }
}

impl Clone for BootSector {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            jumps: self.jumps,
            oem_name: self.oem_name,
            ebpb: self.ebpb,
            boot_code: self.boot_code,
            boot_signature: self.boot_signature,
        }
    }
}

impl Default for BootSector {
    #[inline]
    fn default() -> Self {
        Self {
            jumps: [0xEB, 0xFE, 0x90],
            oem_name: [0; 8],
            ebpb: ExtendedBpb::default(),
            boot_code: [0; 0x1C0],
            boot_signature: Self::BOOT_SIGNATURE,
        }
    }
}

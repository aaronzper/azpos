use core::{ascii, mem};

use super::FATType;

#[repr(u8)]
#[non_exhaustive]
pub enum FATMediaType {
    Fixed = 0xF8,
    Removable = 0xF0,
}

/// A FAT boot record
#[repr(C, packed)]
pub struct FATBootRecord {
    pub jmp_boot: [u8; 3],
    pub oem_name: [ascii::Char; 8],
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sector_count: u16,
    pub num_fats: u8,
    pub root_entry_count: u16,
    total_sectors_16: u16,
    pub media_type: FATMediaType,
    fat_size_16: u16,
    pub sectors_per_track: u16,
    pub num_heads: u16,
    pub hidden_sectors: u32,
    total_sectors_32: u32,
    ebr: ExtendedBootRecordUnion,
}

impl FATBootRecord {
    /// Parses out a FAT Boot Record from a boot sector. Given sector must be
    /// at least 512 bytes, returns `None` if not.
    pub fn new(boot_sector: &[u8]) -> Option<Self> {
        let sized: &[u8; 512] = boot_sector.try_into().ok()?;
        Some(unsafe { mem::transmute_copy(sized) })
    }

    /// Gives the total number of sectors on the media
    pub fn total_sectors(&self) -> u32 {
        if self.total_sectors_16 != 0 {
            self.total_sectors_16 as u32
        } else {
            self.total_sectors_32
        }
    }

    /// Gives the number of clusters on the media
    pub fn cluster_count(&self) -> u32 {
        let root_dir_sectors = 
            ((self.root_entry_count * 32) + (self.bytes_per_sector - 1))
            / self.bytes_per_sector;

        let fat_size = if self.fat_size_16 != 0 {
            self.fat_size_16 as u32
        } else {
            // Manually read this instead of using `extended_boot_record()`
            // since we need the cluster count to calculate the FAT type, and
            // the FAT type to determine which EBR to use. This is safe since
            // if fat_size_16 is 0, we must be usint FAT32.
            unsafe { self.ebr.fat32.fat_size_32 }
        };

        let data_sectors = self.total_sectors() - (
            self.reserved_sector_count as u32 + 
            (self.num_fats as u32 * fat_size) + 
            root_dir_sectors as u32
        );

        data_sectors / self.sectors_per_cluster as u32
    }

    /// Gives the type of the file sytem: FAT12/16/32
    pub fn fat_type(&self) -> FATType {
        if self.root_entry_count == 0 || self.fat_size_16 == 0 {
            return FATType::Fat32;
        }

        let cluster_count = self.cluster_count();
        if cluster_count < 4085 {
            FATType::Fat12
        } else if cluster_count < 65525 {
            FATType::Fat16
        } else {
            FATType::Fat32
        }
    }

    /// Gives the size of the FAT in sectors
    pub fn fat_size(&self) -> u32 {
        match self.extended_boot_record() {
            ExtendedBootRecord::Fat32(ebr) => ebr.fat_size_32,
            ExtendedBootRecord::Legacy(_) => self.fat_size_16 as u32,
        }
    }

    /// Gives the extended boot record
    pub fn extended_boot_record<'a>(&'a self) -> ExtendedBootRecord<'a> {
        match self.fat_type() {
            FATType::Fat12 | FATType::Fat16 => unsafe {
                ExtendedBootRecord::Legacy(&self.ebr.legacy)
            },

            FATType::Fat32 => unsafe {
                ExtendedBootRecord::Fat32(&self.ebr.fat32)
            }
        }
    }

    /// Returns true if the signature word in the EBR is as expected, and false
    /// if not
    pub fn valid_signature(&self) -> bool {
        let signature = match self.extended_boot_record() {
            ExtendedBootRecord::Legacy(ebr) => ebr.signature_word,
            ExtendedBootRecord::Fat32(ebr) => ebr.signature_word,
        };

        signature == 0xAA55
    }
}

pub enum ExtendedBootRecord<'a> { 
    Legacy(&'a LegacyEBR), 
    Fat32(&'a FAT32EBR),
}

#[repr(C)]
union ExtendedBootRecordUnion {
    pub legacy: LegacyEBR,
    pub fat32: FAT32EBR,
}

/// The Extended Boot Record for FAT12 and 16
#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct LegacyEBR {
    pub drive_num: u8,
    reserved_0: u8,
    pub boot_signature: u8,
    pub volume_id: u32,
    pub volume_label: [ascii::Char; 11],
    pub file_sys_type: [ascii::Char; 8],
    reserved_1: [u8; 448],
    pub signature_word: u16,
}

/// The Extended Boot Record for FAT32
#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct FAT32EBR {
    fat_size_32: u32,
    pub flags: u16,
    pub version: u16,
    pub root_cluster: u32,
    pub fs_info: u16,
    pub backup_boot_sector: u16,
    reserved_0: [u8; 12],
    pub drive_num: u8,
    reserved_1: u8,
    pub boot_signature: u8,
    pub volume_id: u32,
    pub volume_label: [ascii::Char; 11],
    pub file_sys_type: [ascii::Char; 8],
    reserved_2: [u8; 420],
    pub signature_word: u16,
}

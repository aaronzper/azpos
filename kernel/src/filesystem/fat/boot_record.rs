use core::{ascii, mem};

use crate::devices::storage::mbr::MBR_SIGNATURE;
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
    /// Parses out a FAT Boot Record from a boot sector. Doesn't garuntee that
    /// the sector is valid!
    pub fn new(bytes: &[u8; 512]) -> Self {
        unsafe { mem::transmute_copy(bytes) }
    }

    /// Gives the total number of sectors on the media
    pub fn total_sectors(&self) -> u32 {
        if self.total_sectors_16 != 0 {
            self.total_sectors_16 as u32
        } else {
            self.total_sectors_32
        }
    }

    /// Gives the number of data sectors
    fn data_sectors(&self) -> u32 {
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

        let non_data_sectors = 
            self.reserved_sector_count as u32 + 
            (self.num_fats as u32 * fat_size) + 
            root_dir_sectors as u32;

        self.total_sectors() - non_data_sectors
    }

    /// Gives the number of clusters on the media
    pub fn cluster_count(&self) -> u32 {
        self.data_sectors() / self.sectors_per_cluster as u32
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
        // Safe since the signature word is in the same position in both types
        // of EBR. Don't want to do this the safe way since this might be an
        // otherwith invalid boot record, so properly checking the FAT type
        // could get messy
        let signature = unsafe {
            self.ebr.legacy.signature_word
        };

        signature == MBR_SIGNATURE
    }

    /// Returns the sector number that starts the given cluster
    pub fn cluster_start_sector(&self, cluster: u32) -> u32 {
        let non_data_sectors = self.total_sectors() - self.data_sectors();
        // Subtract 2 cause the first two cluster numbers are reserved/invalid
        non_data_sectors + ((cluster - 2) * self.sectors_per_cluster as u32)
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

/// The other FAT test modules use `Geometry` to build boot sectors.
#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::ktest::*;

    /// The fields of a boot sector that the cases set
    #[derive(Clone, Copy)]
    pub struct Geometry {
        pub bytes_per_sector: u16,
        pub sectors_per_cluster: u8,
        pub reserved: u16,
        pub num_fats: u8,
        pub root_entries: u16,
        pub total_16: u16,
        pub fat_size_16: u16,
        pub total_32: u32,
        pub fat_size_32: u32,
        pub root_cluster: u32,
        pub signature: u16,
    }

    impl Geometry {
        /// A FAT32 volume with 98368 clusters of one sector
        pub const FAT32: Self = Self {
            bytes_per_sector: 512,
            sectors_per_cluster: 1,
            reserved: 32,
            num_fats: 2,
            root_entries: 0,
            total_16: 0,
            fat_size_16: 0,
            total_32: 100_000,
            fat_size_32: 800,
            root_cluster: 2,
            signature: MBR_SIGNATURE,
        };

        /// A legacy volume with `clusters` clusters of one sector.
        /// It has 1 reserved sector, 1 FAT of `fat_size` sectors and 32 root
        /// directory sectors.
        pub fn legacy(clusters: u32, fat_size: u16) -> Self {
            let total = 1 + fat_size as u32 + 32 + clusters;
            let (total_16, total_32) = match u16::try_from(total) {
                Ok(t) => (t, 0),
                Err(_) => (0, total),
            };
            Self {
                reserved: 1,
                num_fats: 1,
                root_entries: 512,
                total_16,
                fat_size_16: fat_size,
                total_32,
                fat_size_32: 0,
                ..Self::FAT32
            }
        }

        /// Gives the raw 512-byte sector. Field offsets follow the FAT
        /// specification.
        pub fn sector(&self) -> [u8; 512] {
            let mut s = [0u8; 512];
            s[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
            s[11..13].copy_from_slice(&self.bytes_per_sector.to_le_bytes());
            s[13] = self.sectors_per_cluster;
            s[14..16].copy_from_slice(&self.reserved.to_le_bytes());
            s[16] = self.num_fats;
            s[17..19].copy_from_slice(&self.root_entries.to_le_bytes());
            s[19..21].copy_from_slice(&self.total_16.to_le_bytes());
            // `FATMediaType` has two variants. Another byte is undefined
            // behavior, so the cases use only 0xF8.
            s[21] = 0xF8;
            s[22..24].copy_from_slice(&self.fat_size_16.to_le_bytes());
            s[32..36].copy_from_slice(&self.total_32.to_le_bytes());
            s[36..40].copy_from_slice(&self.fat_size_32.to_le_bytes());
            s[44..48].copy_from_slice(&self.root_cluster.to_le_bytes());
            s[510..512].copy_from_slice(&self.signature.to_le_bytes());
            s
        }

        pub fn record(&self) -> FATBootRecord {
            FATBootRecord::new(&self.sector())
        }
    }

    ktest! {
        fn new_reads_each_field_from_its_offset() {
            let g = Geometry { sectors_per_cluster: 4, root_cluster: 7, ..Geometry::FAT32 };
            let br = g.record();
            check_eq!({ br.bytes_per_sector }, 512);
            check_eq!(br.sectors_per_cluster, 4);
            check_eq!({ br.reserved_sector_count }, 32);
            check_eq!(br.num_fats, 2);
            check_eq!({ br.root_entry_count }, 0);
            check_eq!(br.total_sectors(), 100_000);
            // SAFETY: the case writes 0xF8, a valid `FATMediaType`.
            let media = unsafe { core::ptr::addr_of!(br.media_type).cast::<u8>().read_unaligned() };
            check_eq!(media, 0xF8);
            check_eq!(br.fat_size(), 800);
            match br.extended_boot_record() {
                ExtendedBootRecord::Fat32(ebr) => check_eq!({ ebr.root_cluster }, 7),
                ExtendedBootRecord::Legacy(_) => fail!("expected a FAT32 EBR"),
            }
        }

        fn total_sectors_prefers_the_16_bit_field() {
            let g = Geometry { total_16: 5000, total_32: 100_000, ..Geometry::FAT32 };
            check_eq!(g.record().total_sectors(), 5000);
            let g = Geometry { total_16: 0, total_32: 100_000, ..Geometry::FAT32 };
            check_eq!(g.record().total_sectors(), 100_000);
        }

        fn fat_type_is_fat32_when_the_root_entry_count_is_0() {
            // The 16-bit FAT size is not 0, so only the root entry count decides.
            let g = Geometry { root_entries: 0, ..Geometry::legacy(100, 12) };
            check_eq!(g.record().fat_type(), FATType::Fat32);
        }

        /// The limits come from the FAT specification.
        fn fat_type_uses_the_cluster_count_limits() {
            let cases = [
                (4084, 12, FATType::Fat12),
                (4085, 12, FATType::Fat16),
                (65524, 256, FATType::Fat16),
                (65525, 256, FATType::Fat32),
            ];
            for (clusters, fat_size, want) in cases {
                let br = Geometry::legacy(clusters, fat_size).record();
                check_eq!(br.cluster_count(), clusters);
                check_eq!(br.fat_type(), want);
            }
        }

        fn fat_size_reads_the_field_of_the_fat_type() {
            check_eq!(Geometry::FAT32.record().fat_size(), 800);
            let g = Geometry { fat_size_32: 999, ..Geometry::legacy(100, 12) };
            check_eq!(g.record().fat_type(), FATType::Fat12);
            check_eq!(g.record().fat_size(), 12);
        }

        /// Cluster 2 starts after the reserved sectors and the FATs.
        fn cluster_2_starts_after_the_fats() {
            check_eq!(Geometry::FAT32.record().cluster_start_sector(2), 32 + 2 * 800);
        }

        fn cluster_start_sector_uses_the_cluster_size() {
            let br = Geometry { sectors_per_cluster: 8, ..Geometry::FAT32 }.record();
            check_eq!(br.cluster_start_sector(2), 1632);
            check_eq!(br.cluster_start_sector(5), 1632 + 3 * 8);
        }

        /// A legacy volume also has root directory sectors before cluster 2.
        fn cluster_start_sector_skips_the_legacy_root_directory() {
            let br = Geometry::legacy(100, 12).record();
            check_eq!(br.cluster_start_sector(2), 1 + 12 + 32);
        }

        fn cluster_count_divides_by_the_cluster_size() {
            let br = Geometry { sectors_per_cluster: 8, ..Geometry::FAT32 }.record();
            check_eq!(br.cluster_count(), 98_368 / 8);
        }

        fn valid_signature_is_true_only_for_aa55() {
            check!(Geometry::FAT32.record().valid_signature());
            for signature in [0x0000, 0x55AA, 0xAA54, 0xFFFF] {
                check!(!Geometry { signature, ..Geometry::FAT32 }.record().valid_signature());
            }
        }
    }
}

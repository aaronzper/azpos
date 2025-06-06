use alloc::{boxed::Box, format};
use boot_record::FATBootRecord;
use fat::{FATEntry, FileAllocationTable};
use crate::{devices::storage::BlockDevice, filesystem::FileSystemError};
use super::{FileMetadata, FileSystem, FileSystemResult};

/// FAT Boot Record structures
mod boot_record;
/// The titular File Allocation Table
mod fat;

/// Types of FAT filesystem
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FATType { Fat12 = 12, Fat16 = 16, Fat32 = 32 }

/// A handle to a FAT filesystem on a particular block device
pub struct FATFilesystem<'a> {
    drive: &'a mut dyn BlockDevice,
    boot_record: FATBootRecord,
    fat: FileAllocationTable,
}

impl<'a> FileSystem<'a> for FATFilesystem<'a> {
    fn mount(drive: &'a mut dyn BlockDevice) -> FileSystemResult<Self> {
        let boot_sector = drive.read_blocks(0, 1).unwrap();
        let boot_record = FATBootRecord::new(&boot_sector).unwrap();

        if boot_record.bytes_per_sector as usize != drive.block_size() {
            return Err(FileSystemError::MountError(
                format!("Invalid sector size: read {}, expected {}",
                    boot_record.bytes_per_sector as usize, drive.block_size())
            ));
        }

        if !boot_record.valid_signature() {
            return Err(FileSystemError::MountError(
                format!("Invalid FAT signature")
            ));
        }

        if boot_record.fat_type() != FATType::Fat32 {
            return Err(FileSystemError::MountError(
                format!("Only FAT32 drives are currently supported")
            ));
        }

        let fat_sector = boot_record.reserved_sector_count as usize;
        let fat_len = boot_record.fat_size() as usize;
        let fat_raw = drive.read_blocks(fat_sector, fat_len).unwrap();
        let fat = FileAllocationTable::new(fat_raw, &boot_record).unwrap();

        println!("Mounted FAT fs!");
        println!("{} clusters at {} sectors per", 
            boot_record.cluster_count(), boot_record.sectors_per_cluster);
        println!("OEM Name: {}", boot_record.oem_name.as_str());
        println!("Type: {:?}", boot_record.fat_type());
        let name = match boot_record.extended_boot_record() {
            boot_record::ExtendedBootRecord::Legacy(ebr) =>
                ebr.volume_label.as_str(),
            boot_record::ExtendedBootRecord::Fat32(ebr) =>
                ebr.volume_label.as_str(),
        };
        println!("Volume Name: {}", name);

        let mut free_count = 0;
        for (i, entry) in fat.iter().enumerate() {
            match entry {
                fat::FATEntry::Free => free_count += 1,
                _ => println!("Cluster {}: {:?}", i, entry),
            }
        }
        println!("{} free clusters", free_count);

        let (first_chain_i, _) = fat.iter()
            .enumerate()
            .filter(|(_, entry)| {
                matches!(entry, FATEntry::Allocated { next: _ })
            })
            .min_by(|(i_a, _), (i_b, _)| i_a.cmp(i_b))
            .unwrap();

        let first_chain = fat.get_chain(first_chain_i as u32).unwrap();
        println!("{:#?}", first_chain);

        Ok(Self { drive, boot_record, fat })
    }

    fn unmount(self) -> &'a mut dyn BlockDevice {
        self.drive
    }

    fn dir_contents(&self, path: &str) -> Box<[FileMetadata]> {
        todo!()
    }
}

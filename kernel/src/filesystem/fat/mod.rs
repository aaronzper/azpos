use alloc::{boxed::Box, format};
use boot_record::FATBootRecord;
use crate::{devices::storage::BlockDevice, filesystem::FileSystemError};
use super::{FileMetadata, FileSystem, FileSystemResult};

/// FAT Boot Record structures
mod boot_record;

/// Types of FAT filesystem
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FATType { Fat12, Fat16, Fat32 }

/// A handle to a FAT filesystem on a particular block device
pub struct FATFilesystem<'a> {
    drive: &'a mut dyn BlockDevice,
}

impl<'a> FileSystem<'a> for FATFilesystem<'a> {
    fn mount(drive: &'a mut dyn BlockDevice) -> FileSystemResult<Self> {
        let boot_sector = drive.read_blocks(0, 1).unwrap();
        let mut boot_record = FATBootRecord::new(&boot_sector).unwrap();

        if boot_record.fat_type() != FATType::Fat32 {
            return Err(FileSystemError::MountError(
                format!("Only FAT32 drives are currently supported")
            ));
        }

        println!("Mounted FAT fs!");
        println!("OEM Name: {}", boot_record.oem_name.as_str());
        println!("Type: {:?}", boot_record.fat_type());
        let name = match boot_record.extended_boot_record() {
            boot_record::ExtendedBootRecord::Legacy(ebr) =>
                ebr.volume_label.as_str(),
            boot_record::ExtendedBootRecord::Fat32(ebr) =>
                ebr.volume_label.as_str(),
        };
        println!("Volume Name: {}", name);

        Ok(Self { drive })
    }

    fn unmount(self) -> &'a mut dyn BlockDevice {
        self.drive
    }

    fn dir_contents(&self, path: &str) -> Box<[FileMetadata]> {
        todo!()
    }
}

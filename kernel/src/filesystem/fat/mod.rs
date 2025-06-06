use core::ascii;

use alloc::{boxed::Box, format, string::String};
use boot_record::FATBootRecord;
use directories::FATDirectory;
use fat::{FATEntry, FileAllocationTable};
use crate::{devices::storage::BlockDevice, filesystem::{FilePath, FileSystemError}};
use super::{FileMetadata, FileSystem, FileSystemResult};

/// FAT Boot Record structures
mod boot_record;
/// The titular File Allocation Table
mod fat;
/// Stuff for dealing with FAT directories
mod directories;

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

impl FATFilesystem<'_> {
    /// Returns the data in the file/directory in the cluster chain started
    /// by the given cluster. Returns `None` if the cluster number is invalid.
    fn read_chain_data(&self, first_cluster: u32) -> Option<Box<[u8]>> {
        let cluster_sz = self.boot_record.sectors_per_cluster as usize;
        let data = self.fat.get_chain(first_cluster)?.iter()
            .flat_map(|cluster| {
                let sector = self.boot_record.cluster_start_sector(*cluster);

                self.drive.read_blocks(sector as usize, cluster_sz).unwrap()
            })
            .collect();

        Some(data)
    }

    /// Returns the directory pointed to by `path`. If the path doesn't exist or
    /// isn't a directory, returns `None`.
    fn get_directory(&self, path: &FilePath) -> Option<FATDirectory> {
        let root_dir_cluster = match self.boot_record.extended_boot_record() {
            boot_record::ExtendedBootRecord::Fat32(ebr) => ebr.root_cluster,
            _ => unimplemented!("FAT 12/16 root dir"),
        };
        let root_data = self.read_chain_data(root_dir_cluster).unwrap();
        let root_dir = FATDirectory::new(root_data).unwrap();
        
        let mut path_iter = path.as_parts();
        let mut dir = root_dir;
        loop {
            let name = match path_iter.next() {
                Some(s) => s.to_uppercase(),
                None => break,
            };

            dir = match dir.find(&name) {
                Some(entry) => {
                    if !entry.attributes.directory() {
                        return None;
                    }

                    let cluster = entry.cluster();
                    let data = self.read_chain_data(cluster)
                        .expect("Invalid cluster number");

                    FATDirectory::new(data).expect("Invalid directory data")
                },

                None => return None,
            }
        }

        Some(dir)
    }
}

impl<'a> FileSystem<'a> for FATFilesystem<'a> {
    fn mount(drive: &'a mut dyn BlockDevice) -> FileSystemResult<Self> {
        let boot_sector = drive.read_blocks(0, 1).unwrap();
        let boot_record = FATBootRecord::new(&(*boot_sector).try_into().unwrap());

        if boot_record.bytes_per_sector as usize != drive.block_size() {
            return Err(FileSystemError::MountError(
                format!("Invalid sector size: read {}, should be {}. Is this formatted?",
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

        let fs = Self { drive, boot_record, fat };
        
        println!("Mounted FAT fs!");
        println!("{} clusters at {} sectors per", 
            fs.boot_record.cluster_count(), fs.boot_record.sectors_per_cluster);
        println!("Type: {:?}", fs.boot_record.fat_type());
        let name = match fs.boot_record.extended_boot_record() {
            boot_record::ExtendedBootRecord::Legacy(ebr) =>
                ebr.volume_label.as_str(),
            boot_record::ExtendedBootRecord::Fat32(ebr) =>
                ebr.volume_label.as_str(),
        };
        println!("Volume Name: {}", name);

        let path = FilePath::new(String::from("/foobar/src")).unwrap();
        println!("Contents of {}:", path.as_str());
        let contents = fs.dir_contents(&path).unwrap();
        for f in contents {
            println!("* {:?}:", f);
            let f_path = FilePath::new(format!("{}/{}", path.as_str(), f.filename)).unwrap();
            let f_data = fs.read_all(&f_path).unwrap();
            let s = f_data.iter()
                .map(|b| match char::try_from(*b) {
                    Ok(c) => c,
                    Err(_) => '?',
                })
                .collect::<String>();
            println!("{s}<EOF>");
        }

        Ok(fs)
    }

    fn unmount(self) -> &'a mut dyn BlockDevice {
        self.drive
    }

    fn dir_contents(&self, path: &FilePath) -> Option<Box<[FileMetadata]>> {
        let files = self.get_directory(path)?.iter()
            .filter_map(|entry| {
                // We'll use filenames starting with a dot as hidden, instead of
                // the FAT attribute, to be filesystem agnostic
                if entry.attributes.hidden() { 
                    return None; 
                }

                let name = match entry.full_name() {
                    Some(n) => String::from(n),
                    None => return None,
                };

                Some(FileMetadata { 
                    filename: name,
                    is_directory: entry.attributes.directory()
                })
            })
            .collect();

        Some(files)
    }

    fn read_all(&self, path: &FilePath) -> Option<Box<[u8]>> {
        let dir = self.get_directory(&path.path_dirs())?;
        let entry = dir.find(&path.filename().to_uppercase())?;

        if entry.attributes.hidden() || entry.full_name().is_none() {
            return None;
        }

        let all_data = self.read_chain_data(entry.cluster())?;
        
        let len = entry.file_size as usize;
        let data = &all_data[0..len];

        Some(data.into())
    }
}

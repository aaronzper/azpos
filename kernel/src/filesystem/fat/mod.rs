
use alloc::{boxed::Box, format, string::String};
use boot_record::FATBootRecord;
use directories::FATDirectory;
use fat::FileAllocationTable;
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

/// These cases read the test disk. The host harness creates the files, see
/// `azpos::fixtures` in the root package.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktest::*;
    use alloc::string::ToString;

    fn path(raw: &str) -> FilePath {
        FilePath::new(raw.to_string()).unwrap()
    }

    ktest! {
        fn the_test_disk_is_fat32() {
            check_eq!(root_fs().boot_record.fat_type(), FATType::Fat32);
        }

        fn read_all_gives_the_contents_of_a_small_file() {
            let data = root_fs().read_all(&path("/tests/hello.txt")).or_fail("read_all")?;
            check_eq!(&*data, b"Hello from the test disk!\n");
        }

        /// The file is longer than one cluster. Byte `i` has the value
        /// `i % 251`.
        fn read_all_follows_the_cluster_chain() {
            let data = root_fs().read_all(&path("/tests/pattern.bin")).or_fail("read_all")?;
            check_eq!(data.len(), 10_000);

            let first_bad = data.iter().enumerate().find(|(i, b)| **b != (*i % 251) as u8);
            check_eq!(first_bad, None);
        }

        fn read_all_finds_a_file_in_nested_directories() {
            let data = root_fs().read_all(&path("/tests/nested/deep/leaf.txt")).or_fail("read_all")?;
            check_eq!(&*data, b"leaf");
        }

        fn the_file_name_ignores_case() {
            check!(root_fs().read_all(&path("/TESTS/Hello.TXT")).is_some());
        }

        fn read_all_of_a_missing_path_gives_none() {
            check!(root_fs().read_all(&path("/tests/missing.txt")).is_none());
            check!(root_fs().read_all(&path("/missing/hello.txt")).is_none());
            // A file is not a directory
            check!(root_fs().read_all(&path("/tests/hello.txt/x")).is_none());
        }

        fn dir_contents_lists_files_and_directories() {
            let entries = root_fs().dir_contents(&path("/tests")).or_fail("dir_contents")?;

            let hello = entries.iter().find(|e| e.filename == "HELLO.TXT").or_fail("HELLO.TXT")?;
            check!(!hello.is_directory);
            let nested = entries.iter().find(|e| e.filename == "NESTED").or_fail("NESTED")?;
            check!(nested.is_directory);
        }

        fn dir_contents_of_a_file_or_a_missing_path_gives_none() {
            check!(root_fs().dir_contents(&path("/tests/hello.txt")).is_none());
            check!(root_fs().dir_contents(&path("/missing")).is_none());
        }

        /// An empty file has no cluster. `read_all` gives an empty buffer.
        /// Today it gives `None`, because cluster 0 is not a valid chain.
        #[known_bug(68)]
        fn read_all_of_an_empty_file_gives_no_bytes() {
            let data = root_fs().read_all(&path("/tests/empty.txt")).or_fail("read_all")?;
            check_eq!(data.len(), 0);
        }

        /// Today `FilePath::path_dirs` panics for a file in the root
        /// directory, so `read_all` panics too.
        #[known_bug(71, crash)]
        fn read_all_reads_a_file_in_the_root_directory() {
            let data = root_fs().read_all(&path("/rootfile.txt")).or_fail("read_all")?;
            check_eq!(&*data, b"root");
        }
    }
}

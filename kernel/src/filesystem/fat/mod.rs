
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
    use alloc::{string::ToString, vec, vec::Vec};
    use boot_record::tests::Geometry;

    fn path(raw: &str) -> FilePath {
        FilePath::new(raw.to_string()).unwrap()
    }

    /// Checks that byte `i` of `data` has the value `i % 251`
    fn is_pattern(data: &[u8]) -> bool {
        data.iter().enumerate().all(|(i, b)| *b == (i % 251) as u8)
    }

    /// Mounts a `RamDisk` on `data` and gives the text of the mount error.
    /// Gives `None` if the mount succeeds.
    fn mount_error(data: Vec<u8>) -> Option<String> {
        let mut disk = RamDisk::new(data);
        match FATFilesystem::mount(&mut disk) {
            Err(FileSystemError::MountError(text)) => Some(text),
            Ok(_) => None,
        }
    }

    /// Builds a disk of `RAM_BLOCK_SIZE` blocks with `sector` in block 0
    fn disk_with_boot_sector(sector: [u8; 512], blocks: usize) -> Vec<u8> {
        let mut data = vec![0u8; blocks * RAM_BLOCK_SIZE];
        data[..512].copy_from_slice(&sector);
        data
    }

    /// Builds one raw 32-byte directory entry with an 8.3 name
    fn raw_entry(name: &[u8; 11], attributes: u8, cluster: u32, size: u32) -> [u8; 32] {
        let mut raw = [0u8; 32];
        raw[0..11].copy_from_slice(name);
        raw[11] = attributes;
        raw[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
        raw[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
        raw[28..32].copy_from_slice(&size.to_le_bytes());
        raw
    }

    const ATTR_DIRECTORY: u8 = 0x10;
    const EOF: u32 = 0x0FFF_FFFF;

    /// The geometry of `small_volume`: 1 FAT of 1 sector and 16 clusters of 1 sector
    fn small_geometry() -> Geometry {
        Geometry { num_fats: 1, fat_size_32: 1, total_32: 32 + 1 + 16, ..Geometry::FAT32 }
    }

    /// Builds a small FAT32 volume. The tree is:
    /// - `/D/A.TXT` (cluster 4): "hello"
    /// - `/D/BIG.TXT` (cluster 5): size 2000, but a chain of one cluster
    /// - `/D/LOST.TXT`: its first cluster (6) is free
    /// - `/D/BADDIR`: a directory with a free first cluster (7)
    fn small_volume() -> Vec<u8> {
        let g = small_geometry();
        let mut data = disk_with_boot_sector(g.sector(), g.total_32 as usize);
        let cluster = |n: usize| (32 + 1 + n - 2) * RAM_BLOCK_SIZE;

        // The FAT starts after the reserved sectors.
        let fat = 32 * RAM_BLOCK_SIZE;
        for (index, value) in [(0, 0x0FFF_FFF8), (1, EOF), (2, EOF), (3, EOF), (4, EOF), (5, EOF)] {
            let offset = fat + index * 4;
            data[offset..offset + 4].copy_from_slice(&u32::to_le_bytes(value));
        }

        let root = cluster(2);
        data[root..root + 32].copy_from_slice(&raw_entry(b"D          ", ATTR_DIRECTORY, 3, 0));

        let d = cluster(3);
        let entries = [
            raw_entry(b"A       TXT", 0, 4, 5),
            raw_entry(b"BIG     TXT", 0, 5, 2000),
            raw_entry(b"LOST    TXT", 0, 6, 3),
            raw_entry(b"BADDIR     ", ATTR_DIRECTORY, 7, 0),
        ];
        for (i, entry) in entries.iter().enumerate() {
            data[d + i * 32..d + (i + 1) * 32].copy_from_slice(entry);
        }

        let a = cluster(4);
        data[a..a + 5].copy_from_slice(b"hello");
        data
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

        fn read_all_of_a_file_of_exactly_one_cluster() {
            let data = root_fs().read_all(&path("/tests/c512.bin")).or_fail("read_all")?;
            check_eq!(data.len(), 512);
            check!(is_pattern(&data));
        }

        fn read_all_of_a_file_of_one_cluster_and_one_byte() {
            let data = root_fs().read_all(&path("/tests/c513.bin")).or_fail("read_all")?;
            check_eq!(data.len(), 513);
            check!(is_pattern(&data));
        }

        /// The directory is longer than one cluster.
        fn dir_contents_lists_all_files_of_a_long_directory() {
            let entries = root_fs().dir_contents(&path("/tests/many")).or_fail("dir_contents")?;
            for i in 0..40 {
                let name = format!("F{i:02}.TXT");
                check!(entries.iter().any(|e| e.filename == name), "{} is missing", name);
            }
        }

        fn read_all_of_the_last_file_of_a_long_directory() {
            let data = root_fs().read_all(&path("/tests/many/f39.txt")).or_fail("read_all")?;
            check_eq!(&*data, b"f39.txt");
        }

        fn dir_contents_of_the_root_directory() {
            let entries = root_fs().dir_contents(&path("/")).or_fail("dir_contents")?;
            for name in ["TESTS", "PROGRAMS"] {
                let entry = entries.iter().find(|e| e.filename == name).or_fail(name)?;
                check!(entry.is_directory, "{} is a directory", name);
            }
            let file = entries.iter().find(|e| e.filename == "ROOTFILE.TXT").or_fail("ROOTFILE.TXT")?;
            check!(!file.is_directory);
        }

        /// A directory is not a file, so `read_all` gives `None`.
        /// Today it gives `Some` with 0 bytes.
        #[known_bug(68)]
        fn read_all_of_a_directory_gives_none() {
            let data = root_fs().read_all(&path("/tests/nested"));
            check!(data.is_none(), "got {:?}", data.map(|d| d.len()));
        }

        /// A disk with no blocks has no boot sector, so `mount` gives `MountError`.
        /// Today `mount` unwraps the read error and panics.
        #[known_bug(62, crash)]
        fn mount_of_an_empty_disk_gives_a_mount_error() {
            check!(mount_error(Vec::new()).is_some());
        }

        fn mount_with_a_bad_signature_gives_a_mount_error() {
            let sector = Geometry { signature: 0, ..Geometry::FAT32 }.sector();
            let text = mount_error(disk_with_boot_sector(sector, 1)).or_fail("mount_error")?;
            check!(text.contains("signature"), "text: {}", text);
        }

        fn mount_with_a_wrong_sector_size_gives_a_mount_error() {
            let sector = Geometry { bytes_per_sector: 4096, ..Geometry::FAT32 }.sector();
            let text = mount_error(disk_with_boot_sector(sector, 1)).or_fail("mount_error")?;
            check!(text.contains("sector size"), "text: {}", text);
        }

        fn mount_of_a_fat16_volume_gives_a_mount_error() {
            let sector = Geometry::legacy(5000, 20).sector();
            let text = mount_error(disk_with_boot_sector(sector, 1)).or_fail("mount_error")?;
            check!(text.contains("FAT32"), "text: {}", text);
        }

        /// The boot sector puts the FAT past the end of the disk.
        /// Today `mount` unwraps the read error and panics.
        #[known_bug(62, crash)]
        fn mount_with_a_fat_past_the_end_gives_a_mount_error() {
            let sector = Geometry::FAT32.sector();
            check!(mount_error(disk_with_boot_sector(sector, 1)).is_some());
        }

        fn mount_of_a_small_volume_reads_a_file() {
            let mut disk = RamDisk::new(small_volume());
            let Ok(fs) = FATFilesystem::mount(&mut disk) else { fail!("mount failed") };
            let data = fs.read_all(&path("/d/a.txt")).or_fail("read_all")?;
            check_eq!(&*data, b"hello");

            let root = fs.dir_contents(&path("/")).or_fail("dir_contents")?;
            check_eq!(root.len(), 1);
            check_eq!(root[0].filename.as_str(), "D");
        }

        fn read_all_of_a_file_with_a_free_first_cluster_gives_none() {
            let mut disk = RamDisk::new(small_volume());
            let Ok(fs) = FATFilesystem::mount(&mut disk) else { fail!("mount failed") };
            check!(fs.read_all(&path("/d/lost.txt")).is_none());
        }

        /// The size in the entry is longer than the chain, so the file is corrupt.
        /// Today `read_all` slices past the end of the data and panics.
        #[known_bug(68, crash)]
        fn read_all_of_a_file_longer_than_its_chain_gives_none() {
            let mut disk = RamDisk::new(small_volume());
            let Ok(fs) = FATFilesystem::mount(&mut disk) else { fail!("mount failed") };
            check!(fs.read_all(&path("/d/big.txt")).is_none());
        }

        /// A directory with an invalid first cluster is corrupt, so the result is `None`.
        /// Today `get_directory` panics with "Invalid cluster number".
        #[known_bug(68, crash)]
        fn dir_contents_of_a_directory_with_a_free_cluster_gives_none() {
            let mut disk = RamDisk::new(small_volume());
            let Ok(fs) = FATFilesystem::mount(&mut disk) else { fail!("mount failed") };
            check!(fs.dir_contents(&path("/d/baddir")).is_none());
        }

        /// The AHCI drive of the root partition reads its boot sector.
        /// The case is here because only this module can read the field `drive`.
        fn the_root_drive_reads_a_fat32_boot_sector() {
            let block = root_fs().drive.read_blocks(0, 1).or_fail("read_blocks")?;
            check_eq!((block[510], block[511]), (0x55, 0xAA));
            check_eq!(&block[0x52..0x5A], b"FAT32   ");
        }

        /// The AHCI drive of the root partition reads more blocks than one command holds.
        /// One command holds `u16::MAX` blocks.
        /// Today the read calls `todo!("Multi-command read")`.
        #[known_bug(60, crash)]
        fn the_root_drive_reads_more_than_one_command() {
            let drive = &root_fs().drive;
            let n = u16::MAX as usize + 1;
            if drive.num_blocks() < n {
                fail!("the drive has only {} blocks", drive.num_blocks());
            }
            let data = drive.read_blocks(0, n).or_fail("read_blocks")?;
            check_eq!(data.len(), n * 512);
        }
    }
}

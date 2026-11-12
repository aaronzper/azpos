//! Disk image construction.
//!
//! `build.rs` includes this file to build the images for `cargo run`. The
//! test harness uses it to build an image for each QEMU boot. Use only `std`
//! and crates that are in both [dependencies] and [build-dependencies].

use std::{fs::{File, OpenOptions}, io::Write, path::Path};

use bootloader::BootConfig;
use fatfs::{FileSystem, FormatVolumeOptions, FsOptions};
use fscommon::StreamSlice;
use mbrman::{MBRPartitionEntry, BOOT_ACTIVE, CHS, MBR};

const SECTOR_SZ: usize = 512;
const DISK_SZ: u64 = 256 * 1024 * 1024; // 256 MiB

/// One file to put in the root file system
pub struct RootFile {
    /// The path in the root file system, with `/` between the components and
    /// no `/` at the start. Example: `Programs/adam.exe`.
    pub path: String,
    pub data: Vec<u8>,
}

impl RootFile {
    pub fn new(path: &str, data: impl Into<Vec<u8>>) -> Self {
        Self { path: path.to_string(), data: data.into() }
    }

    /// Reads the contents from a file on the host
    pub fn from_host(path: &str, host_path: &Path) -> Self {
        let data = std::fs::read(host_path)
            .unwrap_or_else(|e| panic!("Cannot read {}: {e}", host_path.display()));
        Self::new(path, data)
    }
}

fn create_root_fs(partition: &mut StreamSlice<File>, files: &[RootFile]) {
    let options = FormatVolumeOptions::new()
        .volume_label(*b"azpOS Root ")
        .fat_type(fatfs::FatType::Fat32);
    fatfs::format_volume(&mut *partition, options).unwrap();

    let fs = FileSystem::new(partition, FsOptions::new()).unwrap();
    let root = fs.root_dir();

    for file in files {
        // Create each parent directory. `create_dir` opens a directory that
        // exists already.
        let mut dir_path = String::new();
        let mut components: Vec<&str> = file.path.split('/').collect();
        components.pop();
        for component in components {
            if !dir_path.is_empty() {
                dir_path.push('/');
            }
            dir_path.push_str(component);
            root.create_dir(&dir_path).unwrap();
        }

        let mut out = root.create_file(&file.path).unwrap();
        out.write_all(&file.data).unwrap();
    }
}

fn create_root_partition_mbr(mut disk: File) -> StreamSlice<File> {
    let mut mbr = MBR::read_from(&mut disk, SECTOR_SZ as u32).unwrap();
    let size = mbr.get_maximum_partition_size().unwrap();
    let start = mbr.find_optimal_place(size).unwrap();
    let part_i = mbr.iter().find(|(_, p)| p.is_unused()).map(|(i, _)| i)
        .unwrap();

    mbr[part_i] = MBRPartitionEntry {
        boot: BOOT_ACTIVE,
        first_chs: CHS::empty(),
        sys: 0x0C,
        last_chs: CHS::empty(),
        starting_lba: start,
        sectors: size,
    };

    mbr.write_into(&mut disk).unwrap();

    let byte_offset = start as u64 * SECTOR_SZ as u64;
    let bytes = size as u64 * SECTOR_SZ as u64;
    StreamSlice::new(disk, byte_offset, bytes).unwrap()
}

/// Creates a BIOS disk image at `image`. The image boots `kernel` and has a
/// FAT32 root partition that contains `root_files`.
pub fn create_bios_image(kernel: &Path, image: &Path, root_files: &[RootFile]) {
    // Leave at default, but we can change later
    let config = BootConfig::default();

    let mut bios_loader = bootloader::BiosBoot::new(kernel);
    bios_loader.set_boot_config(&config);
    bios_loader.create_disk_image(image).unwrap();

    let bios_img = OpenOptions::new().read(true).write(true).open(image)
        .unwrap();
    bios_img.set_len(DISK_SZ).unwrap();
    let mut bios_root_part = create_root_partition_mbr(bios_img);
    create_root_fs(&mut bios_root_part, root_files);
}

/// Creates a UEFI disk image at `image` that boots `kernel`. The image has
/// no root partition.
pub fn create_uefi_image(kernel: &Path, image: &Path) {
    let config = BootConfig::default();

    let mut uefi_loader = bootloader::UefiBoot::new(kernel);
    uefi_loader.set_boot_config(&config);
    uefi_loader.create_disk_image(image).unwrap();
}

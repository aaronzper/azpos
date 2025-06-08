use std::{fs::{self, File, OpenOptions}, io::Write, path::{Path, PathBuf}};

use bootloader::BootConfig;
use fatfs::{FileSystem, FormatVolumeOptions, FsOptions};
use fscommon::{BufStream, StreamSlice};
use mbrman::{MBRPartitionEntry, BOOT_ACTIVE, CHS, MBR};

const SECTOR_SZ: usize = 512;
const DISK_SZ: u64 = 256 * 1024 * 1024; // 256 MiB

fn create_root_fs(partition: &mut StreamSlice<File>) {
    let options = FormatVolumeOptions::new()
        .volume_label(*b"azpOS Root\0")
        .fat_type(fatfs::FatType::Fat32);
    fatfs::format_volume(&mut *partition, options).unwrap();

    let fs = FileSystem::new(partition, FsOptions::new()).unwrap();

    fs.root_dir().create_dir("Programs").unwrap();

    let testprog_path = 
        PathBuf::from(std::env::var_os("CARGO_BIN_FILE_ADAM_adam").unwrap());
    let testprog = fs::read(testprog_path).unwrap();
    let mut file = fs.root_dir().create_file("Programs/adam.exe").unwrap();
    file.write_all(&testprog).unwrap();
}

fn create_root_parition_mbr(mut disk: File) -> StreamSlice<File> {
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
    let partition = StreamSlice::new(disk, byte_offset, bytes).unwrap();

    partition
}

fn main() {
    // set by cargo, build scripts should use this directory for output files
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());

    // set by cargo's artifact dependency feature, see
    // https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#artifact-dependencies
    let kernel = PathBuf::from(std::env::var_os("CARGO_BIN_FILE_AZPOS_KERNEL_azpos-kernel").unwrap());
    println!("Build kernel at {}", kernel.display());

    // Leave at default, but we can change later
    let config = BootConfig::default();

    // create an UEFI disk image
    let uefi_path = out_dir.join("uefi.img");
    let mut uefi_loader = bootloader::UefiBoot::new(&kernel);
    uefi_loader.set_boot_config(&config);
    uefi_loader.create_disk_image(&uefi_path).unwrap();

    // create a BIOS disk image
    let bios_path = out_dir.join("bios.img");
    let mut bios_loader = bootloader::BiosBoot::new(&kernel);
    bios_loader.set_boot_config(&config);
    bios_loader.create_disk_image(&bios_path).unwrap();

    let bios_img = OpenOptions::new().read(true).write(true).open(&bios_path)
        .unwrap();
    bios_img.set_len(DISK_SZ).unwrap(); // 256 MiB
    let mut bios_root_part = create_root_parition_mbr(bios_img);
    create_root_fs(&mut bios_root_part);

    // pass the disk image paths as env variables to the `main.rs`
    println!("cargo:rustc-env=UEFI_PATH={}", uefi_path.display());
    println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());
}

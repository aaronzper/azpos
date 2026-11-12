//! Host checks of the repo and of the disk image builder. They do not start
//! QEMU.

use std::{fs::File, io::Read, path::{Path, PathBuf}};

use azpos::{fixtures, image::{self, RootFile}};
use fatfs::{FileSystem, FsOptions};
use fscommon::StreamSlice;
use mbrman::MBR;

fn repo_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Each module of `systest` with cases is in `MODULES` of `main.rs`.
#[test]
fn each_systest_module_with_cases_is_in_modules() {
    let src = repo_dir().join("programs/systest/src");
    let main = std::fs::read_to_string(src.join("main.rs")).unwrap();

    let mut missing = Vec::new();
    for entry in std::fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_str().unwrap();
        if stem == "main" {
            continue;
        }

        let text = std::fs::read_to_string(&path).unwrap();
        if text.contains("cases!") && !main.contains(&format!("{stem}::CASES")) {
            missing.push(stem.to_string());
        }
    }

    assert!(missing.is_empty(), "Add these modules to MODULES in main.rs: {missing:?}");
}

/// Removes the file when this value drops
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn read_file(fs: &FileSystem<StreamSlice<File>>, path: &str) -> Vec<u8> {
    let mut data = Vec::new();
    fs.root_dir().open_file(path)
        .unwrap_or_else(|e| panic!("Cannot open {path}: {e}"))
        .read_to_end(&mut data)
        .unwrap();
    data
}

/// The root partition of a BIOS image contains each fixture file.
#[test]
fn a_bios_image_contains_each_fixture_file() {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let image_path = TempFile(dir.join(format!("azpos-host-test-{pid}.img")));

    // `build.rs` built this kernel already, so the test costs no build.
    let kernel: &Path = &azpos::artifacts::kernel_elf();

    let mut files = fixtures::files();
    files.push(RootFile::new("Programs/extra.bin", vec![7u8; 3]));
    image::create_bios_image(kernel, &image_path.0, &files);

    let mut disk = File::open(&image_path.0).unwrap();
    let mbr = MBR::read_from(&mut disk, 512).unwrap();
    // The bootloader adds its own FAT partition first. The root partition
    // is the last used entry.
    let (_, part) = mbr.iter().filter(|(_, p)| p.is_used()).last().expect("No partition");
    let start = part.starting_lba as u64 * 512;
    let len = part.sectors as u64 * 512;

    let fs = FileSystem::new(StreamSlice::new(disk, start, start + len).unwrap(), FsOptions::new())
        .unwrap();

    for file in &files {
        assert_eq!(read_file(&fs, &file.path), file.data, "{}", file.path);
    }
}

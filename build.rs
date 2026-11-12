use std::path::PathBuf;

use image::RootFile;

#[path = "src/image.rs"]
mod image;

/// Returns the path of a binary that an artifact dependency built. Cargo
/// sets the variable, see
/// https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#artifact-dependencies
fn artifact(var: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(var).unwrap_or_else(|| panic!("{var} is not set")))
}

/// The binaries of `programs/testprogs`. `artifacts::test_programs` in
/// `src/lib.rs` has the same list.
const TEST_PROGRAMS: [&str; 8] = ["yield", "logloop", "spin", "ud2", "nullrd", "div0", "wrkern", "hlt"];

fn main() {
    // set by cargo, build scripts should use this directory for output files
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());

    let kernel = artifact("CARGO_BIN_FILE_AZPOS_KERNEL_azpos-kernel");
    let adam = artifact("CARGO_BIN_FILE_ADAM_adam");
    let systest = artifact("CARGO_BIN_FILE_SYSTEST_systest");
    println!("Build kernel at {}", kernel.display());

    // create an UEFI disk image
    let uefi_path = out_dir.join("uefi.img");
    image::create_uefi_image(&kernel, &uefi_path);

    // create a BIOS disk image
    let bios_path = out_dir.join("bios.img");
    let root_files = [RootFile::from_host("Programs/adam.exe", &adam)];
    image::create_bios_image(&kernel, &bios_path, &root_files);

    // pass the disk image paths as env variables to the `main.rs`
    println!("cargo:rustc-env=UEFI_PATH={}", uefi_path.display());
    println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());

    // The test harness builds its own images from these files
    println!("cargo:rustc-env=AZPOS_KERNEL_ELF={}", kernel.display());
    println!("cargo:rustc-env=AZPOS_ADAM_ELF={}", adam.display());
    println!("cargo:rustc-env=AZPOS_SYSTEST_ELF={}", systest.display());
    // The fixture programs of the kernel tests
    for name in TEST_PROGRAMS {
        let path = artifact(&format!("CARGO_BIN_FILE_TESTPROGS_{name}"));
        println!("cargo:rustc-env=AZPOS_TESTPROG_{name}={}", path.display());
    }
    println!("cargo:rustc-env=AZPOS_WORK_DIR={}", out_dir.display());
}

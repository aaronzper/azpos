//! Host tools for azpOS: disk images, QEMU, and the test harness.
//!
//! The binaries and tests of this package use this library:
//! - `src/main.rs`: `cargo run` boots azpOS in QEMU.
//! - `tests/ktest.rs`: `cargo ktest` runs the kernel tests, then the user
//!   test program, on the test kernel.
//! - `tests/e2e.rs`: `cargo e2e` runs the user test program on the
//!   production kernel.

/// Command-line options of the test binaries
pub mod cli;
/// Files that the harness puts on the test disk
pub mod fixtures;
/// Disk image construction
pub mod image;
/// The rules that turn the raw result of a case into its outcome
pub mod outcome;
/// The QEMU command line
pub mod qemu;
/// The loop that boots QEMU and reads the test protocol
pub mod session;

use std::path::PathBuf;

/// Builds the test kernel with a nested cargo call
pub mod test_kernel;

/// The files that `build.rs` built or found
pub mod artifacts {
    use super::PathBuf;

    /// The production kernel
    pub fn kernel_elf() -> PathBuf {
        PathBuf::from(env!("AZPOS_KERNEL_ELF"))
    }

    /// The init process
    pub fn adam_elf() -> PathBuf {
        PathBuf::from(env!("AZPOS_ADAM_ELF"))
    }

    /// The user test program
    pub fn systest_elf() -> PathBuf {
        PathBuf::from(env!("AZPOS_SYSTEST_ELF"))
    }

    /// The fixture programs of the kernel tests, as pairs of name and path.
    /// `build.rs` has the same list of names.
    pub fn test_programs() -> [(&'static str, PathBuf); 8] {
        macro_rules! prog {
            ($name:literal) => {
                ($name, PathBuf::from(env!(concat!("AZPOS_TESTPROG_", $name))))
            };
        }
        [
            prog!("yield"), prog!("logloop"), prog!("spin"), prog!("ud2"),
            prog!("nullrd"), prog!("div0"), prog!("wrkern"), prog!("hlt"),
        ]
    }

    /// A directory for files that the harness creates
    pub fn work_dir() -> PathBuf {
        PathBuf::from(env!("AZPOS_WORK_DIR"))
    }
}

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

    /// A directory for files that the harness creates
    pub fn work_dir() -> PathBuf {
        PathBuf::from(env!("AZPOS_WORK_DIR"))
    }
}

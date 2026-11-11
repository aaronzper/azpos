//! The files that the kernel file system tests read. The harness puts them
//! in the root file system of each test disk.
//!
//! The kernel tests in `kernel/src/filesystem/fat/mod.rs` contain the
//! expected contents. Change both places together.

use crate::image::RootFile;

/// The length of `Tests/pattern.bin`. The file is longer than one cluster.
pub const PATTERN_LEN: usize = 10_000;

/// The contents of `Tests/pattern.bin`: byte `i` has the value `i % 251`.
/// The period is a prime number, so no cluster has the same contents as its
/// neighbor.
pub fn pattern() -> Vec<u8> {
    (0..PATTERN_LEN).map(|i| (i % 251) as u8).collect()
}

/// Returns the fixture files
pub fn files() -> Vec<RootFile> {
    vec![
        RootFile::new("rootfile.txt", "root"),
        RootFile::new("Tests/hello.txt", "Hello from the test disk!\n"),
        RootFile::new("Tests/empty.txt", ""),
        RootFile::new("Tests/pattern.bin", pattern()),
        RootFile::new("Tests/nested/deep/leaf.txt", "leaf"),
    ]
}

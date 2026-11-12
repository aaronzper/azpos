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

/// The number of files in `Tests/many`. The directory is longer than one
/// cluster.
pub const MANY_COUNT: usize = 40;

/// Returns the fixture files
pub fn files() -> Vec<RootFile> {
    let mut files = vec![
        RootFile::new("rootfile.txt", "root"),
        RootFile::new("Tests/hello.txt", "Hello from the test disk!\n"),
        RootFile::new("Tests/empty.txt", ""),
        RootFile::new("Tests/pattern.bin", pattern()),
        RootFile::new("Tests/nested/deep/leaf.txt", "leaf"),
        // A file of exactly one cluster, and a file of one cluster and one byte
        RootFile::new("Tests/c512.bin", pattern()[..512].to_vec()),
        RootFile::new("Tests/c513.bin", pattern()[..513].to_vec()),
    ];

    // Each file in `Tests/many` contains its own name, for example `f07.txt`.
    for i in 0..MANY_COUNT {
        let name = format!("f{i:02}.txt");
        files.push(RootFile::new(&format!("Tests/many/{name}"), name.clone()));
    }

    files
}

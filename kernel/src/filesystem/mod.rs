use alloc::{boxed::Box, string::String};

use crate::devices::storage::BlockDevice;

/// Metadata on a file or directory, but not its contents
pub struct FileMetadata {
    filename: String,
    is_directory: bool,
}

/// A file system that can be mounted from a block device, and supports a
/// standard file system interface
pub trait FileSystem {
    /// Consumes a `BlockDevice` to mount the filesystem to it
    fn mount(drive: impl BlockDevice) -> Self;

    /// Unmounts the filesystem, consuming itself and returning its inner drive
    fn unmount(self) -> impl BlockDevice;
    
    /// Provides the `FileMetadata` of every entry in a particular directory
    fn dir_contents(&self, path: &str) -> Box<[FileMetadata]>;

    // TODO: File R/W, creation, moving, etc
}

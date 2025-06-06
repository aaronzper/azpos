use alloc::{boxed::Box, string::String};
use crate::devices::storage::BlockDevice;

/// File path struct
mod path;
pub use path::FilePath;

/// FAT filesystem implementation
pub mod fat;

/// Metadata on a file or directory, but not its contents
pub struct FileMetadata {
    filename: String,
    is_directory: bool,
}

/// A file system that can be mounted from a block device, and supports a
/// standard file system interface
pub trait FileSystem<'a> {

    /// Consumes a `BlockDevice` to mount the filesystem to it
    fn mount(drive: &'a mut dyn BlockDevice) -> FileSystemResult<Self>
        where Self: Sized;

    /// Unmounts the filesystem, consuming itself and returning its inner drive
    fn unmount(self) -> &'a mut dyn BlockDevice;
    
    /// Provides the `FileMetadata` of every entry in a particular directory
    fn dir_contents(&self, path: &FilePath) -> Box<[FileMetadata]>;

    // TODO: File R/W, creation, moving, etc
}

pub type FileSystemResult<T> = Result<T, FileSystemError>;

#[derive(Debug)]
pub enum FileSystemError {
    /// Couldnt mount the given block device
    MountError(String),
}

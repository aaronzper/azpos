use alloc::{boxed::Box, string::String};
use crate::devices::storage::BlockDevice;

/// File path struct
mod path;
pub use path::FilePath;

/// FAT filesystem implementation
pub mod fat;

#[derive(Debug)]
/// Metadata on a file or directory, but not its contents
pub struct FileMetadata {
    pub filename: String,
    pub is_directory: bool,
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
    ///
    /// Returns `None` if the path is invalid (doesn't exist, not a dir, etc)
    fn dir_contents(&self, path: &FilePath) -> Option<Box<[FileMetadata]>>;

    /// Reads all data in the file at the given path, if it exists
    fn read_all(&self, path: &FilePath) -> Option<Box<[u8]>>;

    // TODO: File R/W, creation, moving, etc
}

pub type FileSystemResult<T> = Result<T, FileSystemError>;

#[derive(Debug)]
pub enum FileSystemError {
    /// Couldnt mount the given block device
    MountError(String),
}

use core::cmp::min;
use alloc::boxed::Box;
use libsci::resources::Resource;

/// A resource encapsulating a finite, generic blob of data that can only be read
///
/// Primarily used by the kernel to return variable-length data from a syscall
/// request (e.g. listing devices or directory entries).
///
/// Only supports reading/seeking. 
pub struct BlobResource {
    data: Box<[u8]>,
    seek_head: usize,
}

impl BlobResource {
    pub fn new(data: Box<[u8]>) -> Self {
        Self { 
            data,
            seek_head: 0,
        }
    }
}

impl Resource for BlobResource {
    fn read(&mut self, buffer: &mut [u8]) -> libsci::resources::ResourceResult {
        let readable = &self.data[self.seek_head..];
        let len = min(buffer.len(), readable.len());
        buffer[..len].clone_from_slice(&readable[..len]);
        self.seek_head += len;
        Ok(len as i64)
    }

    fn seek(&mut self, offset: usize) -> libsci::resources::ResourceResult {
        self.seek_head = offset;
        Ok(0)
    }

    fn write(&mut self, _: &[u8]) -> libsci::resources::ResourceResult {
        Err(libsci::resources::ResourceError::Unsupported)
    }
}

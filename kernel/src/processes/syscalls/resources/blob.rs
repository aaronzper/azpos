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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktest::*;
    use libsci::resources::ResourceError;

    fn blob(data: &[u8]) -> BlobResource {
        BlobResource::new(data.into())
    }

    ktest! {
        fn read_gives_the_data_in_pieces_then_zero() {
            let mut blob = blob(b"abcdefg");
            let mut buf = [0u8; 3];

            check_matches!(blob.read(&mut buf), Ok(3));
            check_eq!(&buf, b"abc");
            check_matches!(blob.read(&mut buf), Ok(3));
            check_eq!(&buf, b"def");
            check_matches!(blob.read(&mut buf), Ok(1));
            check_eq!(buf[0], b'g');
            check_matches!(blob.read(&mut buf), Ok(0));
        }

        fn read_into_an_empty_buffer_reads_nothing() {
            let mut blob = blob(b"abc");
            check_matches!(blob.read(&mut []), Ok(0));

            let mut buf = [0u8; 3];
            check_matches!(blob.read(&mut buf), Ok(3));
            check_eq!(&buf, b"abc");
        }

        fn seek_moves_the_read_position() {
            let mut blob = blob(b"abcdefg");
            let mut buf = [0u8; 2];

            check_matches!(blob.seek(5), Ok(0));
            check_matches!(blob.read(&mut buf), Ok(2));
            check_eq!(&buf, b"fg");

            check_matches!(blob.seek(0), Ok(0));
            check_matches!(blob.read(&mut buf), Ok(2));
            check_eq!(&buf, b"ab");
        }

        fn seek_to_the_end_then_read_gives_zero() {
            let mut blob = blob(b"abc");
            let mut buf = [0u8; 2];

            check_matches!(blob.seek(3), Ok(0));
            check_matches!(blob.read(&mut buf), Ok(0));
        }

        fn write_is_unsupported() {
            let mut blob = blob(b"abc");
            check_matches!(blob.write(b"x"), Err(ResourceError::Unsupported));
        }

        /// A read after a seek past the end returns 0 bytes, or the seek
        /// returns an error. Today the read panics on a slice index.
        #[known_bug(44, crash)]
        fn read_after_seek_past_the_end_does_not_panic() {
            let mut blob = blob(b"abc");
            let mut buf = [0u8; 2];

            if blob.seek(4).is_ok() {
                check_matches!(blob.read(&mut buf), Ok(0));
            }
        }
    }
}

//! A block device in memory for test cases

use alloc::{boxed::Box, vec::Vec};

use crate::devices::storage::{BlockDevice, BlockDeviceError, BlockDeviceResult};

/// The block size of a `RamDisk` in bytes
pub const RAM_BLOCK_SIZE: usize = 512;

/// A `BlockDevice` on a `Vec<u8>` with 512-byte blocks.
/// A length that is not a multiple of 512 loses its last partial block.
pub struct RamDisk {
    data: Vec<u8>,
}

impl RamDisk {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl BlockDevice for RamDisk {
    fn num_blocks(&self) -> usize {
        self.data.len() / RAM_BLOCK_SIZE
    }

    fn block_size(&self) -> usize {
        RAM_BLOCK_SIZE
    }

    fn read_blocks(&self, index: usize, n_blocks: usize) -> BlockDeviceResult<Box<[u8]>> {
        let end = index.checked_add(n_blocks).ok_or(BlockDeviceError::InvalidBlock)?;
        if end > self.num_blocks() {
            return Err(BlockDeviceError::InvalidBlock);
        }
        Ok(self.data[index * RAM_BLOCK_SIZE..end * RAM_BLOCK_SIZE].into())
    }

    fn write_blocks(&mut self, index: usize, data: &[u8]) -> BlockDeviceResult<()> {
        if data.len() % RAM_BLOCK_SIZE != 0 {
            return Err(BlockDeviceError::LengthNotBlockMultiple);
        }
        let end = index + data.len() / RAM_BLOCK_SIZE;
        if end > self.num_blocks() {
            return Err(BlockDeviceError::InvalidBlock);
        }
        self.data[index * RAM_BLOCK_SIZE..end * RAM_BLOCK_SIZE].copy_from_slice(data);
        Ok(())
    }

    /// A `RamDisk` has no partitions.
    fn partition(self) -> Option<Box<[Box<dyn BlockDevice>]>> {
        None
    }
}

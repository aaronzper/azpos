use alloc::boxed::Box;

/// AHCI (SATA) driver
pub mod ahci;

/// General trait for any block storage device that reads and writes in units
/// of blocks
pub trait BlockDevice {
    /// The number of blocks on the device
    fn num_blocks(&self) -> usize;
    /// The size, in bytes, of a single block
    fn block_size(&self) -> usize;
    /// Reads `n_blocks` blocks starting at a certain block index.
    ///
    /// Blocks until the operation is complete.
    fn read_blocks(&mut self, index: usize, n_blocks: usize) 
        -> BlockDeviceResult<Box<[u8]>>;
    /// Writes the given data starting at the given block index. Data size must
    /// be a multiple of `self.block_size()`.
    ///
    /// Blocks until the operation is complete.
    fn write_blocks(&mut self, index: usize, data: &[u8]) -> BlockDeviceResult<()>;
}

pub type BlockDeviceResult<T> = Result<T, BlockDeviceError>;

#[derive(Debug)]
pub enum BlockDeviceError {
    
}

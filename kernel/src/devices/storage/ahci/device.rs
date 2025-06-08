use core::cmp::min;
use alloc::{boxed::Box, slice, sync::Arc, vec::Vec};
use bitvec::{bitbox, boxed::BitBox};
use x86_64::structures::paging::PhysFrame;
use crate::{devices::storage::{ahci::{ata::{ATACommand, ATADriveInfo}, fis::FISRegisterH2D, mmio::PRDTEntry, PRDT_ENTRIES_PER_COMMAND}, mbr::MasterBootRecord, BlockDevice, BlockDeviceError, BlockDeviceResult}, memory::{dealloc_frame, mmio::alloc_mmio_block}, scheduling::threads::sync::{KCondvar, KMutex, KMutexGuard}};
use super::mmio::{AHCICommandHeader, AHCIPort};

struct DeviceState {
    /// The underlying MMIO register for this AHCI port
    port: &'static mut AHCIPort,
    /// Tracks whether command slot `i` is available (true) or in use (false)
    available_commands: BitBox,
}

/// Stores data that is shared between multiple "devices" on the same
/// physical AHCI device (specifically, partitions). The condvar is used
/// for waiting on a command slot to be available.
struct Shared {
    /// Mutable state of the device, as opposed to the non-mut metadata stored
    /// in the other fields. See `DeviceState` for more info.
    state: KMutex<DeviceState>,

    /// Condvar for waiting for a command to become available
    avail_commands_cv: KCondvar,
}

/// An individual AHCI device that can be written to or read from, or a
/// partition thereof
pub struct AHCIDevice {
    shared: Arc<Shared>,

    /// Basic info on the (physical) device (not the partiton)
    info: ATADriveInfo,

    /// The LBA sector the device starts at. Used for partitioning.
    start_sector: usize,
    /// The number of sectors the device contains. Used for partitioning.
    sectors: usize,
}

impl AHCIDevice {
    /// Constructs a new AHCI device, given a reference to the HBA port
    /// register. All pre-reqresuite data structures (the command list,
    /// command tables, FISs, etc) should already be set up and allocatedf.
    pub fn new(port: &'static mut AHCIPort, n_commands: usize) -> Self {
        let commands = port.command_list(n_commands);

        let bc = 512;

        commands[0].prdt_entries = 1;
        commands[0].command_table().command_fis = 
            FISRegisterH2D::new_with_type()
            .with_is_command(true)
            .with_command(ATACommand::IDENTIFY_DEVICE);

        let (buf_ptr, buf) = unsafe { alloc_mmio_block::<u16>(bc).unwrap() };

        commands[0].command_table().prdt[0].set_byte_count(bc as u32);
        commands[0].command_table().prdt[0].set_int_flag(true);
        commands[0].command_table().prdt[0].set_addr(buf.start.start_address());

        port.start();

        port.issue_command(0);
        while port.command_busy(0) { }

        let data = unsafe { slice::from_raw_parts(buf_ptr, 256) };
        let info = ATADriveInfo::new(data.try_into().unwrap());

        for frame in buf {
            dealloc_frame(frame);
        }

        let state = KMutex::new(DeviceState {
            port,
            available_commands: bitbox![1; n_commands],
        });

        Self { 
            shared: Arc::new(Shared { state, avail_commands_cv: KCondvar::new() }),

            start_sector: 0,
            sectors: info.sectors(),

            info,
        }
    }

    /// The number of blocks/sectors that can be fit in a single command for
    /// reading or writing
    pub fn blocks_per_command(&self) -> usize {
        const BYTES_PER_COMMAND: usize = PRDTEntry::MAX_DATA_SIZE as usize 
            * PRDT_ENTRIES_PER_COMMAND as usize;
        
        // ATA spec recquires 16-bit count value, so cant be more than that
        min(BYTES_PER_COMMAND / self.block_size(), u16::MAX as usize)
    }

    /// Allocates an unused command slot. Blocks until one is available
    fn allocate_command<'a>(&'a self) -> CommandSlot<'a> {
        let mut state = self.shared.state.lock();
        loop {
            match state.available_commands.first_one() {
                Some(i) => {    
                    state.available_commands.set(i, false);

                    break CommandSlot {
                        slot: i,
                        state,
                        device: self,
                        buffer: None,
                        alloced_frames: Vec::new(),
                    };
                },
                None => {
                    state = self.shared.avail_commands_cv.wait(state);
                },
            }
        }
    }
}

struct CommandSlot<'a> {
    /// The actual command slot we're using
    slot: usize,
    /// The state of the device on which the command applies
    state: KMutexGuard<'a, DeviceState>,
    /// The device itself
    device: &'a AHCIDevice,
    /// Whether we've set up a AHCI PRDT buffer and, if so, how long it is
    buffer: Option<usize>,
    /// All physical frames allocated by this command for the buffer
    alloced_frames: Vec<PhysFrame>,
}

impl CommandSlot<'_> {
    fn command_header(&mut self) -> &mut AHCICommandHeader {
        let n_commands = self.state.available_commands.len();
        &mut self.state.port.command_list(n_commands)[self.slot]
    }

    fn execute(&mut self) {
        self.state.port.issue_command(self.slot);

        // TODO: Block here
        while self.state.port.command_busy(self.slot) { }
    }

    fn setup_command(&mut self, command: ATACommand, block: usize, count: usize)
        -> BlockDeviceResult<()> {

        if count > self.device.sectors {
            return Err(BlockDeviceError::InvalidBlock);
        }

        let block_actual = block + self.device.start_sector;

        const MASK: usize = (1usize << 24) - 1;
        let lba_low = block_actual & MASK;
        let lba_high = (block_actual >> 24) & MASK;

        let cmd_fis = FISRegisterH2D::new_with_type()
            .with_is_command(true)
            .with_command(command)
            .with_device(1 << 6) // Enable LBA (linear) addressing
            .with_lba_low(lba_low as u32)
            .with_lba_high(lba_high as u32)
            .with_count(count as u16);

        self.command_header().flags.set_write(command == ATACommand::WRITE_DMA_EXT);
        self.command_header().command_table().command_fis = cmd_fis;
        self.setup_buffer(count * self.device.block_size())?;

        Ok(())
    }

    fn setup_buffer(&mut self, len_bytes: usize) -> BlockDeviceResult<()> {
        if self.buffer.is_some() {
            panic!("There's already a buffer set up on this command slot!");
        }

        let prdt_entires = (len_bytes + PRDTEntry::MAX_DATA_SIZE as usize - 1) 
            / PRDTEntry::MAX_DATA_SIZE as usize;

        let cmd_h = self.command_header();
        cmd_h.prd_bytes_trans = 0;
        cmd_h.prdt_entries = prdt_entires as u16;

        let cmd_t = cmd_h.command_table();

        let mut alloced_frames = Vec::new();
        for prdt_i in 0..prdt_entires {
            let prdt_data_len = {
                let len_so_far = PRDTEntry::MAX_DATA_SIZE as usize * prdt_i;
                let len_to_go = len_bytes - len_so_far;
                min(len_to_go, PRDTEntry::MAX_DATA_SIZE as usize)
            };

            let prdt = &mut cmd_t.prdt[prdt_i];
            prdt.set_byte_count(prdt_data_len as u32);
            prdt.set_int_flag(true);

            let (_, block) = unsafe { 
                match alloc_mmio_block::<u8>(prdt_data_len) {
                    Some(x) => x,
                    None => return Err(BlockDeviceError::OutOfMemory),
                }
            };

            prdt.set_addr(block.start.start_address());

            alloced_frames.extend(block);
        }

        self.alloced_frames = alloced_frames;
        self.buffer = Some(len_bytes);
        Ok(())
    }

    /// Copies data out from the buffer. Panics if the buffer isnt set up yet.
    /// Assumes this is ran after a DMA transfer
    fn copy_from_buffer(&mut self) -> Box<[u8]> {
        let mut out_buf = Vec::with_capacity(self.buffer.unwrap());

        for i in 0..self.command_header().prdt_entries {
            let prdt = &mut self.command_header().command_table().prdt[i as usize];
            let prdt_buf = prdt.get_mut_buf();

            out_buf.extend_from_slice(prdt_buf);
        }

        out_buf.into()
    }

    /// Copies data into the buffer. Panics if it isnt set up yet
    fn copy_to_buffer(&mut self, data: &[u8]) {
        for i in 0..self.command_header().prdt_entries {
            let prdt = &mut self.command_header().command_table().prdt[i as usize];
            let prdt_buf = prdt.get_mut_buf();

            let start = i as usize * PRDTEntry::MAX_DATA_SIZE as usize;
            let end = start + prdt_buf.len();
            prdt_buf.copy_from_slice(&data[start..end]);
        }
    }
}

impl Drop for CommandSlot<'_> {
    fn drop(&mut self) {
        for frame in &self.alloced_frames {
            dealloc_frame(*frame);
        }

        self.state.available_commands.set(self.slot, true);
    }
}

impl BlockDevice for AHCIDevice {
    fn num_blocks(&self) -> usize {
        self.info.sectors()
    }

    fn block_size(&self) -> usize {
        self.info.sector_size()
    }

    fn read_blocks(&self, index: usize, n_blocks: usize)
        -> BlockDeviceResult<Box<[u8]>> {

        if n_blocks == 0 {
            return Ok(Box::default());
        }

        if n_blocks > self.blocks_per_command() {
            todo!("Multi-command read")
        }

        let mut command = self.allocate_command();
        command.setup_command(ATACommand::READ_DMA_EXT, index, n_blocks)?;
        command.execute();

        // Check that we got the amount of bytes we asked for
        if command.buffer.unwrap() as u32 != command.command_header().prd_bytes_trans {
            return Err(BlockDeviceError::OperationFailed {
                transferred: command.command_header().prd_bytes_trans as usize,
                expected: command.buffer.unwrap(),
            });
        }

        Ok(command.copy_from_buffer())
    }

    fn write_blocks(&mut self, index: usize, data: &[u8]) -> BlockDeviceResult<()> {
        if data.len() == 0 {
            return Ok(());
        }

        if data.len() % self.block_size() != 0 {
            return Err(BlockDeviceError::LengthNotBlockMultiple);
        }

        let n_blocks = data.len() / self.block_size();

        if n_blocks > self.blocks_per_command() {
            todo!("Multi-command write")
        }

        let mut command = self.allocate_command();
        command.setup_command(ATACommand::WRITE_DMA_EXT, index, n_blocks)?;
        command.copy_to_buffer(data);
        command.execute();

        // Check that we sent the amount of bytes we asked for
        if command.buffer.unwrap() as u32 != command.command_header().prd_bytes_trans {
            return Err(BlockDeviceError::OperationFailed {
                transferred: command.command_header().prd_bytes_trans as usize,
                expected: command.buffer.unwrap(),
            });
        }

        Ok(())
    }

    fn partition(self) -> Option<Box<[Box<dyn BlockDevice>]>> {
        let mbr_bytes: Box<[u8; 512]> = self.read_blocks(0, 1).unwrap()
            .try_into().unwrap();
        let mbr = MasterBootRecord::new(&mbr_bytes)?;
        
        let mut partitions: Vec<Box<dyn BlockDevice>> = Vec::new();
        for entry in mbr.active_partitions() {
            let device = AHCIDevice {
                shared: Arc::clone(&self.shared),

                start_sector: entry.lba_start() as usize,
                sectors: entry.num_sectors() as usize,

                info: self.info.clone(),
            };

            partitions.push(Box::new(device));
        }

        Some(partitions.into_boxed_slice())
    }
}

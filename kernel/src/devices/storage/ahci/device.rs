use core::cmp::min;
use alloc::{boxed::Box, slice, vec::Vec};
use bitvec::{bitbox, boxed::BitBox};
use x86_64::structures::paging::PhysFrame;
use crate::{devices::storage::{ahci::{ata::{ATACommand, ATADriveInfo}, fis::FISRegisterH2D, mmio::PRDTEntry, PRDT_ENTRIES_PER_COMMAND}, BlockDevice, BlockDeviceResult}, memory::{dealloc_frame, mmio::alloc_mmio_block}, scheduling::threads::sync::{KCondvar, KMutex}};
use super::mmio::{AHCICommandHeader, AHCIPort};

/// An individual AHCI device that can be written to or read from
pub struct AHCIDevice {
    /// The underlying MMIO register for this AHCI port
    port: &'static mut AHCIPort,
    /// The number of parralel commands supported
    n_commands: usize,

    /// Tracks whether command slot `i` is available (true) or in use (false)
    available_commands: KMutex<BitBox>,
    /// Condvar for waiting for a command to become available
    avail_commands_cv: KCondvar,

    /// Basic info on the device
    info: ATADriveInfo,
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

        let available_commands = {
            let bitbox = bitbox![1; n_commands];
            KMutex::new(bitbox)
        };

        Self { 
            port, n_commands, 

            available_commands, avail_commands_cv: KCondvar::new(),

            info
        }
    }

    fn command_list(&mut self) -> &mut [AHCICommandHeader] {
        self.port.command_list(self.n_commands)
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
    fn allocate_command<'a>(&'a mut self) -> CommandSlot<'a> {
        let mut lock = self.available_commands.lock();
        loop {
            match lock.first_one() {
                Some(i) => {    
                    lock.set(i, false);
                    drop(lock);

                    break CommandSlot {
                        slot: i,
                        device: self,
                        buffer: None,
                        alloced_frames: Vec::new(),
                    };
                },
                None => {
                    lock = self.avail_commands_cv.wait(lock);
                },
            }
        }
    }
}

struct CommandSlot<'a> {
    /// The actual command slot we're using
    slot: usize,
    /// The device on which the command applies
    device: &'a mut AHCIDevice,
    /// Whether we've set up a AHCI PRDT buffer and, if so, how long it is
    buffer: Option<usize>,
    /// All physical frames allocated by this command for the buffer
    alloced_frames: Vec<PhysFrame>,
}

impl CommandSlot<'_> {
    fn command_header(&mut self) -> &mut AHCICommandHeader {
        &mut self.device.command_list()[self.slot]
    }

    fn execute(&mut self) {
        self.device.port.issue_command(self.slot);

        // TODO: Block here
        while self.device.port.command_busy(self.slot) { }
    }

    fn setup_command(&mut self, command: ATACommand, block: usize, count: usize) {
        const MASK: usize = (1usize << 24) - 1;
        let lba_low = block & MASK;
        let lba_high = (block >> 24) & MASK;

        let cmd_fis = FISRegisterH2D::new_with_type()
            .with_is_command(true)
            .with_command(command)
            .with_device(1 << 6) // Enable LBA (linear) addressing
            .with_lba_low(lba_low as u32)
            .with_lba_high(lba_high as u32)
            .with_count(count as u16);

        self.command_header().flags.set_write(command == ATACommand::WRITE_DMA_EXT);
        self.command_header().command_table().command_fis = cmd_fis;
        self.setup_buffer(count * self.device.block_size());
    }

    fn setup_buffer(&mut self, len_bytes: usize) {
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
                alloc_mmio_block::<u8>(prdt_data_len)
                .expect("Could not allocate block for PRDT buffer")
            };

            prdt.set_addr(block.start.start_address());

            alloced_frames.extend(block);
        }

        self.alloced_frames = alloced_frames;
        self.buffer = Some(len_bytes);
    }

    /// Copies data out from the buffer. Panics if the buffer isnt set up yet.
    /// Assumes this is ran after a DMA transfer
    fn copy_from_buffer(&mut self) -> Box<[u8]> {
        let mut out_buf = Vec::with_capacity(self.buffer.unwrap());

        // Sanity check that we got the amount of bytes we asked for
        assert_eq!(
            self.buffer.unwrap() as u32, 
            self.command_header().prd_bytes_trans);

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

        self.device.available_commands.lock().set(self.slot, true);
    }
}

impl BlockDevice for AHCIDevice {
    fn num_blocks(&self) -> usize {
        self.info.sectors()
    }

    fn block_size(&self) -> usize {
        self.info.sector_size()
    }

    fn read_blocks(&mut self, index: usize, n_blocks: usize)
        -> BlockDeviceResult<Box<[u8]>> {

        if n_blocks == 0 {
            return Ok(Box::default());
        }

        if n_blocks > self.blocks_per_command() {
            todo!("Multi-command read")
        }

        let mut command = self.allocate_command();
        command.setup_command(ATACommand::READ_DMA_EXT, index, n_blocks);
        command.execute();

        Ok(command.copy_from_buffer())
    }

    fn write_blocks(&mut self, index: usize, data: &[u8]) -> BlockDeviceResult<()> {
        if data.len() == 0 {
            return Ok(());
        }

        assert!(data.len() % self.block_size() == 0);

        let n_blocks = data.len() / self.block_size();

        if n_blocks > self.blocks_per_command() {
            todo!("Multi-command write")
        }

        let mut command = self.allocate_command();
        command.setup_command(ATACommand::WRITE_DMA_EXT, index, n_blocks);
        command.copy_to_buffer(data);
        command.execute();

        Ok(())
    }
}

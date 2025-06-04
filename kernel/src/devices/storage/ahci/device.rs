use alloc::{boxed::Box, slice, vec};
use bitvec::{bitbox, boxed::BitBox};
use crate::{devices::storage::ahci::{ata::{ATACommand, ATADriveInfo}, fis::FISRegisterH2D}, memory::{dealloc_frame, mmio::alloc_mmio_block}, scheduling::threads::sync::KMutex};
use super::mmio::{AHCICommandHeader, AHCIPort};

#[derive(Debug)]
/// An individual AHCI device that can be written to or read from
pub struct AHCIDevice {
    /// The underlying MMIO register for this AHCI port
    port: &'static mut AHCIPort,
    /// The number of parralel commands supported
    n_commands: usize,
    /// Tracks whether command slot `i` is available (true) or in use (false)
    available_commands: KMutex<BitBox>,
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
        commands[0].command_table().prdt[0].set_addr(buf_ptr);

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

        Self { port, n_commands, available_commands, info }
    }

    fn command_list(&mut self) -> &mut [AHCICommandHeader] {
        self.port.command_list(self.n_commands)
    }
}

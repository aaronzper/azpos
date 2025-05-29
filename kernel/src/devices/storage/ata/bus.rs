use alloc::string::{String, ToString};
use x86_64::structures::port::{PortRead, PortWrite};

use super::{commands::ATACommand, ports::{read_port, write_port, DriveHeadRegister, ErrorRegister, IORegisterType, StatusRegister}};

#[derive(Debug, Copy, Clone)]
pub enum ATABusDrive { Primary = 0, Secondary = 1 }

#[derive(Debug)]
pub struct ATABus {
    io_base: u16,
    control_base: u16,
    selected_drive: ATABusDrive,
    selected_block_address: u64,
    has_primary: bool,
    has_secondary: bool,
}

impl ATABus {
    pub fn new(io_base: u16, control_base: u16) -> Option<Self> {
        let mut bus = Self {
            io_base,
            control_base,
            selected_drive: ATABusDrive::Primary,
            selected_block_address: 0,
            has_primary: false,
            has_secondary: false,
        };

        // Floating bus indicates no drives
        if bus.status().floating_bus() {
            return None;
        }

        bus.check_for_drive(ATABusDrive::Primary);
        bus.check_for_drive(ATABusDrive::Secondary);

        if !bus.has_primary && !bus.has_secondary {
            return None;
        }

        Some(bus)
    }

    /// Checks if the given drive exists on the bus, and updates internal state
    /// accordingly
    fn check_for_drive(&mut self, drive: ATABusDrive) {
        self.select_drive(drive);
        self.write_io(IORegisterType::SectorCount, 0u16);
        self.write_io(IORegisterType::LBALow, 0u16);
        self.write_io(IORegisterType::LBAMid, 0u16);
        self.write_io(IORegisterType::LBAHigh, 0u16);
        self.send_command(ATACommand::IDENTIFY);

        let drive_exists = if self.status().zero() {
            false
        } else {
            // Poll until BSY clears
            while self.status().busy() {}

            if self.read_io::<u8>(IORegisterType::LBAMid) != 0 
                || self.read_io::<u8>(IORegisterType::LBAHigh) != 0 {
                false
            } else {
                loop {
                    let status = self.status();
                    
                    if status.error() {
                        break false;
                    }

                    if status.data_request() {
                        let mut buf = [0u16; 256];
                        for i in 0..buf.len() {
                            buf[i] = self.read_io(IORegisterType::Data);
                        }

                        // Safe since buf[100..104] is defined as a u64 with the
                        // number of LBA48 sectors
                        let num_sectors = unsafe {
                            *(&raw const buf[100] as *const u64)
                        };
                        //
                        // Safe since buf[108..112] is defined as a string
                        let mut device_name = String::from_iter(
                            buf[27..47].iter()
                            .flat_map(|x| x.to_be_bytes())
                            .map(|x| x as char)
                        );
                        device_name = device_name.trim_end().to_string();

                        println!("Detected drive {} with {} sectors", device_name, num_sectors);

                        break true;
                    }
                }
            }
        };

        match drive {
            ATABusDrive::Primary => self.has_primary = drive_exists,
            ATABusDrive::Secondary => self.has_secondary = drive_exists,
        }
    }

    fn read_io<T: PortRead + core::fmt::Debug>(&self, reg: IORegisterType) -> T {
        let offset = self.io_base + reg as u16;
        read_port(offset)
    }

    fn write_io<T: PortWrite>(&self, reg: IORegisterType, val: T) {
        let offset = self.io_base + reg as u16;
        write_port(offset, val);
    }

    fn select_drive(&mut self, drive: ATABusDrive) {
        // Set the LBA bits to 0 since we're using LBA48 here, which doesnt
        // set these bits (only LBA28 does)
        let dh_reg = DriveHeadRegister::new(0, drive);
        self.write_io(IORegisterType::DriveHead, dh_reg);
        
        // Poll Status 14 times to wait for it to update
        for _ in 0..14 { self.status(); }

        self.selected_drive = drive;
    }

    fn select_block(&mut self, block_address: u64) {
        if block_address >= 2^48 {
            panic!("Block address too large: {}", block_address);
        }
    }

    fn send_command(&mut self, command: ATACommand) {
        self.write_io(IORegisterType::StatusCommand, command);
    }

    fn error(&self) -> ErrorRegister {
        self.read_io(IORegisterType::ErrorFeatures)
    }

    fn status(&self) -> StatusRegister {
        self.read_io(IORegisterType::StatusCommand)
    }
}

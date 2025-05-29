use bus::ATABus;
use ports::{PRIMARY_IO_OFFSET, PRIMARY_CONTROL_OFFSET, SECONDARY_IO_OFFSET, SECONDARY_CONTROL_OFFSET};

/// I/O port definitions used to control ATA devices
mod ports;
/// ATA command definitions
mod commands;
/// ATA bus control
mod bus;

#[derive(Debug)]
pub struct ATAController {
    primary_bus: Option<ATABus>,
    secondary_bus: Option<ATABus>,
}

impl ATAController {
    pub fn new() -> Self {
        Self {
            primary_bus: 
                ATABus::new(PRIMARY_IO_OFFSET, PRIMARY_CONTROL_OFFSET),
            secondary_bus:
                ATABus::new(SECONDARY_IO_OFFSET, SECONDARY_CONTROL_OFFSET),
        }
    }
}

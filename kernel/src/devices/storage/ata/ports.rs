use x86_64::{instructions::port::{PortReadOnly, PortWriteOnly}, structures::port::{PortRead, PortWrite}};

use super::bus::ATABusDrive;

pub const PRIMARY_IO_OFFSET: u16 = 0x1F0;
pub const SECONDARY_IO_OFFSET: u16 = 0x170;
pub const PRIMARY_CONTROL_OFFSET: u16 = 0x3F6;
pub const SECONDARY_CONTROL_OFFSET: u16 = 0x376;

pub fn read_port<T: PortRead>(port: u16) -> T {
    let mut p: PortReadOnly<T> =
        x86_64::instructions::port::PortGeneric::new(port);
    unsafe {
        p.read()
    }
}

pub fn write_port<T: PortWrite>(port: u16, value: T) {
    let mut p: PortWriteOnly<T> =
        x86_64::instructions::port::PortGeneric::new(port);
    unsafe {
        p.write(value)
    }
}

#[derive(Copy, Clone, Debug)]
pub enum IORegisterType {
    Data = 0,
    ErrorFeatures,
    SectorCount,
    LBALow,
    LBAMid,
    LBAHigh,
    DriveHead,
    StatusCommand,
}

#[derive(Copy, Clone, Debug)]
pub enum ControlRegisterType {
    AlternateStatusDeviceControl = 0,
    DriveAddress,
}

#[derive(Copy, Clone, Debug)]
pub struct ErrorRegister(u8);

impl ErrorRegister {
    const AMNF: u8  = 1 << 0;
    const TKZNF: u8 = 1 << 1;
    const ABRT: u8  = 1 << 2;
    const MCR: u8   = 1 << 3;
    const IDNF: u8  = 1 << 4;
    const MC: u8    = 1 << 5;
    const UNC: u8   = 1 << 6;
    const BBK: u8   = 1 << 7;

    pub fn addr_mark_not_found(self) -> bool {
        self.0 & Self::AMNF != 0
    }

    pub fn track_zero_not_found(self) -> bool {
        self.0 & Self::TKZNF != 0
    }

    pub fn aborted_command(self) -> bool {
        self.0 & Self::ABRT != 0
    }

    pub fn media_change_request(self) -> bool {
        self.0 & Self::MCR != 0
    }

    pub fn id_not_found(self) -> bool {
        self.0 & Self::IDNF != 0
    }

    pub fn media_changed(self) -> bool {
        self.0 & Self::MC != 0
    }

    pub fn uncorrectable_data(self) -> bool {
        self.0 & Self::UNC != 0
    }

    pub fn bad_block_detected(self) -> bool {
        self.0 & Self::BBK != 0
    }
}

impl PortRead for ErrorRegister {
    unsafe fn read_from_port(port: u16) -> Self {
        Self(unsafe { u8::read_from_port(port) })
    }
}

#[derive(Copy, Clone, Debug)]
pub struct StatusRegister(u8);

impl StatusRegister {
    const ERR: u8   = 1 << 0;
    const IDX: u8   = 1 << 1;
    const CORR: u8  = 1 << 2;
    const DRQ: u8   = 1 << 3;
    const SRV: u8   = 1 << 4;
    const DF: u8    = 1 << 5;
    const RDY: u8   = 1 << 7;
    const BSY: u8   = 1 << 7;

    pub fn zero(self) -> bool {
        self.0 == 0
    }

    pub fn floating_bus(self) -> bool {
        self.0 == 0xFF
    }

    pub fn error(self) -> bool {
        self.0 & Self::ERR != 0
    }

    pub fn index(self) -> bool {
        self.0 & Self::IDX != 0
    }

    pub fn corrected_data(self) -> bool {
        self.0 & Self::CORR != 0
    }

    pub fn data_request(self) -> bool {
        self.0 & Self::DRQ != 0
    }

    pub fn service_request(self) -> bool {
        self.0 & Self::SRV != 0
    }

    pub fn drive_fault(self) -> bool {
        self.0 & Self::DF != 0
    }

    pub fn ready(self) -> bool {
        self.0 & Self::RDY != 0
    }

    pub fn busy(self) -> bool {
        self.0 & Self::BSY != 0
    }
}

impl PortRead for StatusRegister {
    unsafe fn read_from_port(port: u16) -> Self {
        Self(unsafe { u8::read_from_port(port) })
    }
}

pub struct DriveHeadRegister(u8);

impl DriveHeadRegister {
    /// Creates a new Drive/Head register value with
    /// - bits 24-27 of the selected LBA-addressed block number (panics if more
    ///   than three bits set here)
    /// - the selected drive 
    pub fn new(block_num_bits_24_27: u8, drive: ATABusDrive) -> Self {
        if block_num_bits_24_27 > 0b1111 {
            panic!("Provided block number bits too big: {:#b}", 
                block_num_bits_24_27);
        }

        let value = 
            // Bits 7 and 5 always set, bit 6 set to use LBA addressing
            // (we dont support CHS addressing)
            0b11100000
            | block_num_bits_24_27
            | ((drive as u8) << 4);

        Self(value)
    }
}

impl PortWrite for DriveHeadRegister {
    unsafe fn write_to_port(port: u16, value: Self) {
        unsafe { u8::write_to_port(port, value.0); }
    }
}

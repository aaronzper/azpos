use alloc::slice;
use modular_bitfield::prelude::*;

#[derive(Copy, Clone, Debug)]
#[derive(Specifier)]
#[bits = 8]
pub enum FISType {
    RegisterH2D     = 0x27,
    RegisterD2H     = 0x34,
    DMAActive       = 0x39,
    DMSSetup        = 0x41,
    Data            = 0x46,
    BISTActivate    = 0x58,
    PIOSetup        = 0x5F,
    SetDeviceBits   = 0xA1,
}

#[derive(Copy, Clone, Debug)]
#[non_exhaustive]
#[derive(Specifier)]
#[bits = 8]
pub enum ATACommand {
    NOP                 = 0x00,
    IDENTIFY_DEVICE     = 0xEC,
}

#[bitfield]
#[repr(C)]
#[derive(Debug)]
pub struct FISRegisterH2D {
    fis_type: FISType,
    pub port_mult_port: B4,
    reserved_0: B3,
    pub is_command: bool,
    pub command: ATACommand,
    pub feature_low: B8,
    pub lba_low: B24,
    pub device: B8,
    pub lba_high: B24,
    pub feature_high: B8,
    pub count: B16,
    pub icc: B8,
    pub control: B8,
    reserved_1: B32,
}

impl FISRegisterH2D {
    pub fn new_with_type() -> Self {
        Self::new().with_fis_type(FISType::RegisterH2D)
    }
}

#[repr(C, packed)]
#[derive(Debug)]
pub struct FISData {
    fis_type: FISType,
    /// Only 4 bits
    pub port_mult_port: u8, 
    reserved: u16,
    // Data would go here but is variable size so we handle it in `get_data`
    // manually
}

impl FISData {
    /// Returns a slice to the data contained by the FIS, given the length
    /// thereof
    ///
    /// Unsafe because if the length is wrong we're cooked
    pub unsafe fn get_data(&self, data_len: usize) -> &[u8] {
        let self_ptr = (self as *const Self) as *const u8;
        unsafe {
            let data_ptr = self_ptr.add(size_of::<Self>());
            slice::from_raw_parts(data_ptr, data_len)
        }
    }
}

#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AHCIDeviceType {
    SATA                = 0x00000101,
    SATAPI              = 0xEB140101,
    EnclosureMgmtBridge = 0xC33C0101,
    PortMultiplier      = 0x96690101,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug)]
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

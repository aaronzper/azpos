pub enum FIS {
    RegisterH2D     = 0x27,
    RegisterD2H     = 0x34,
    DMAActive       = 0x39,
    DMSSetup        = 0x41,
    Data            = 0x46,
    BISTActivate    = 0x58,
    PIOSetup        = 0x5F,
    SetDeviceBits   = 0xA1,
}

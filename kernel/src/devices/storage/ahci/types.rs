#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum AHCIDeviceType {
    SATA                = 0x00000101,
    SATAPI              = 0xEB140101,
    EnclosureMgmtBridge = 0xC33C0101,
    PortMultiplier      = 0x96690101,
}

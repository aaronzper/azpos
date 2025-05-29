#[derive(Debug)]
pub enum PCIDeviceClass {
    Unclassifed,
    MassStorageCtrl,
    NetworkCtrl,
    DisplayCtrl,
    MultimediaCtrl,
    MemoryCtrl,
    Bridge,
    SimpleCommCtrl,
    BaseSysPeripheral,
    InputDeviceCtrl,
    DockingStation,
    Processor,
    SerialBusCtrl,
    WirelessCtrl,
    IntelligentCtrl,
    SatCommCtrl,
    EncryptionCtrl,
    SignalProcCtrl,
    ProcAccelorator,
    NonEssential,
    CoProcessor,
    Unassigned
}

impl TryFrom<u8> for PCIDeviceClass {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x0 => Ok(Self::Unclassifed),
            0x1 => Ok(Self::MassStorageCtrl),
            0x2 => Ok(Self::NetworkCtrl),
            0x3 => Ok(Self::DisplayCtrl),
            0x4 => Ok(Self::MultimediaCtrl),
            0x5 => Ok(Self::MemoryCtrl),
            0x6 => Ok(Self::Bridge),
            0x7 => Ok(Self::SimpleCommCtrl),
            0x8 => Ok(Self::BaseSysPeripheral),
            0x9 => Ok(Self::InputDeviceCtrl),
            0xA => Ok(Self::DockingStation),
            0xB => Ok(Self::Processor),
            0xC => Ok(Self::SerialBusCtrl),
            0xD => Ok(Self::WirelessCtrl),
            0xE => Ok(Self::IntelligentCtrl),
            0xF => Ok(Self::SatCommCtrl),
            0x10 => Ok(Self::EncryptionCtrl),
            0x11 => Ok(Self::SignalProcCtrl),
            0x12 => Ok(Self::ProcAccelorator),
            0x13 => Ok(Self::NonEssential),
            0x40 => Ok(Self::CoProcessor),
            0xFF => Ok(Self::Unassigned),

            _ => Err(()),
        }
    }
}

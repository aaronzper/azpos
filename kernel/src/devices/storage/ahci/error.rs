#[derive(Debug)]
pub enum AHCIError {
    /// Attempted to create an AHCI controller from a PCI device with the wrong
    /// class/subclass
    WrongPCIDevice,
    /// Attempted to create an AHCI controller from an HBA that doesn't support
    /// 64-bit addressing
    DoesntSupport64BitAddr,
}

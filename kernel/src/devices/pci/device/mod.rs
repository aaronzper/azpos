use class::PCIDeviceClass;

use super::config::read_config;

pub mod class;

#[derive(Debug)]
pub struct PCIDevice {
    bus: u8,
    device: u8,
    function: u8,
    class: PCIDeviceClass,
    subclass: u8,
}

impl PCIDevice {
    pub fn new(bus: u8, device: u8, function: u8) -> Option<Self> {
        if device > 0b11111 {
            panic!("Device number must be 5 bits (< 32)");
        }

        if function > 0b111 {
            panic!("Function number must be 3 bits (< 8)");
        }

        let mut dev = Self { 
            bus, device, function,
            class: PCIDeviceClass::Unclassifed,
            subclass: 0,
        };

        if dev.vendor_id() == 0xFFFF {
            None
        } else {
            let word_2 = dev.read_config(2).to_le_bytes();
            dev.class = word_2[3].try_into().unwrap();
            dev.subclass = word_2[2];

            Some(dev)
        }
    }

    /// Reads a specific 32-bit word from the config space of the device at the
    /// given index. 
    pub fn read_config(&self, word: u8) -> u32 {
        read_config(self.bus, self.device, self.function, word)
    }

    /// Returns the vendor ID of the PCI device
    pub fn vendor_id(&self) -> u16 {
        self.read_config(0) as u16
    }

    /// Returns the class and subclass of the device
    pub fn class(&self) -> (PCIDeviceClass, u8) {
        (self.class, self.subclass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::pci::PCIController;
    use crate::devices::storage::ahci::SATA_PCI_SUBCLASS;
    use crate::ktest::*;
    use alloc::vec::Vec;

    ktest! {
        /// QEMU puts the Intel host bridge at bus 0, device 0, function 0.
        fn the_scan_finds_the_host_bridge_first() {
            let pci = PCIController::new();
            let first = pci.devices().first().or_fail("no PCI device")?;
            check_eq!((first.bus, first.device, first.function), (0, 0, 0));
            check_eq!(first.vendor_id(), 0x8086);
            check_eq!(first.class(), (PCIDeviceClass::Bridge, 0x00));
        }

        fn the_scan_finds_exactly_one_sata_controller() {
            let pci = PCIController::new();
            let count = pci.devices().iter()
                .filter(|d| d.class() == (PCIDeviceClass::MassStorageCtrl, SATA_PCI_SUBCLASS))
                .count();
            check_eq!(count, 1);
        }

        /// The scan lists each function one time, in the order of its address.
        fn the_scan_lists_each_function_once_in_order() {
            let pci = PCIController::new();
            let addrs: Vec<(u8, u8, u8)> = pci.devices().iter().map(|d| (d.bus, d.device, d.function)).collect();
            check!(addrs.windows(2).all(|w| w[0] < w[1]), "addresses: {addrs:?}");
        }

        /// No device answers at the last address of bus 0.
        fn new_of_an_empty_address_gives_none() {
            check!(PCIDevice::new(0, 31, 7).is_none());
        }

        fn class_from_a_known_code() {
            check_eq!(PCIDeviceClass::try_from(0x01), Ok(PCIDeviceClass::MassStorageCtrl));
            check_eq!(PCIDeviceClass::try_from(0x40), Ok(PCIDeviceClass::CoProcessor));
            check_eq!(PCIDeviceClass::try_from(0xFF), Ok(PCIDeviceClass::Unassigned));
        }

        fn class_from_a_reserved_code_gives_an_error() {
            check_eq!(PCIDeviceClass::try_from(0x14), Err(()));
            check_eq!(PCIDeviceClass::try_from(0x3F), Err(()));
            check_eq!(PCIDeviceClass::try_from(0x41), Err(()));
        }

        /// The rustdoc of the `config` module says that a device number of 32 or more panics.
        #[should_panic("Device number must be 5 bits")]
        fn new_with_a_device_number_of_32_panics() {
            let _ = PCIDevice::new(0, 32, 0);
        }

        /// The rustdoc of the `config` module says that a function number of 8 or more panics.
        #[should_panic("Function number must be 3 bits")]
        fn new_with_a_function_number_of_8_panics() {
            let _ = PCIDevice::new(0, 0, 8);
        }
    }
}

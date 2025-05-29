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

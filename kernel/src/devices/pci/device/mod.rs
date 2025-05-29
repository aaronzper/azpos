use class::PCIDeviceClass;

use super::config::read_config;

mod class;

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

    fn read_config(&self, word: u8) -> u32 {
        read_config(self.bus, self.device, self.function, word)
    }

    pub fn vendor_id(&self) -> u16 {
        self.read_config(0) as u16
    }
}

use alloc::{boxed::Box, vec::Vec};
use device::PCIDevice;

/// Utilities for reading from and writing to the PCI configuration space for
/// PCI devices.
///
/// Specific parameters for functions (panics if not met):
/// - `device` must be at most 5 bits
/// - `func` must be at most 3 bits
/// - `word` must be an index into the 32-bit words in the configuration
///    space. Must be below 64, as that's how many words there are.
mod config;
mod device;

#[derive(Debug)]
pub struct PCIController {
    devices: Box<[PCIDevice]>,
}

impl PCIController {
    pub fn new() -> PCIController {
        let mut devices = Vec::new();
        for b in 0..u8::MAX {
            for d in 0..32 {
                for f in 0..8 {
                    if let Some(device) = PCIDevice::new(b, d, f) {
                        devices.push(device);
                    }
                }
            }
        }

        Self {
            devices: devices.into(),
        }
    }
}

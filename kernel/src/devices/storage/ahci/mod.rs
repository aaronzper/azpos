use alloc::{alloc::alloc, boxed::Box, vec::Vec};
use mmio::{AHCIBaseMemoryReg, AHCIPort};
use bitvec::{order::Lsb0, view::BitView};
use x86_64::{PhysAddr, VirtAddr};

use crate::{devices::pci::{PCIDevice, PCIDeviceClass}, memory::resolve_phys_addr};

/// AHCI Memory-Mapped IO structures
mod mmio;
/// AHCI device types
mod types;

pub const SATA_PCI_SUBCLASS: u8 = 0x6;
const PCI_BAR_5: u8 = 0x9;

#[derive(Debug)]
pub struct AHCIController {
    base_mem_register: &'static mut AHCIBaseMemoryReg,
    good_ports: Box<[usize]>,
}

impl AHCIController {
    pub fn new(pci_device: &PCIDevice) -> Self {
        let (class, subclass) = pci_device.class();
        if class != PCIDeviceClass::MassStorageCtrl || subclass != SATA_PCI_SUBCLASS {
            panic!("PCI device incorrectly had class {:?} and subclass {}",
                class, subclass);
        }

        let bmr_pa = pci_device.read_config(PCI_BAR_5);
        let bmr_ptr: *mut AHCIBaseMemoryReg =
            resolve_phys_addr(PhysAddr::new(bmr_pa as u64))
            .unwrap()
            .as_mut_ptr();
        let bmr = unsafe { bmr_ptr.as_mut().unwrap() };

        let good_ports = bmr.port_implemented.view_bits::<Lsb0>()
            .iter_ones()
            .collect();

        Self {
            base_mem_register: bmr,
            good_ports,
        }
    }
}

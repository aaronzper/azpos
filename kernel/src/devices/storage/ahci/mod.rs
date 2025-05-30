use alloc::boxed::Box;
use error::AHCIError;
use mmio::AHCIBaseMemoryReg;
use bitvec::{order::Lsb0, view::BitView};
use x86_64::PhysAddr;

use crate::{devices::pci::{PCIDevice, PCIDeviceClass}, memory::resolve_phys_addr};

/// AHCI Memory-Mapped IO structures
mod mmio;
/// AHCI device types
mod types;
/// AHCI errors
mod error;

pub const SATA_PCI_SUBCLASS: u8 = 0x6;
const PCI_BAR_5: u8 = 0x9;

#[derive(Debug)]
pub struct AHCIController {
    base_mem_register: &'static mut AHCIBaseMemoryReg,
    good_ports: Box<[usize]>,
}

impl AHCIController {
    pub fn new(pci_device: &PCIDevice) -> Result<Self, AHCIError> {
        let (class, subclass) = pci_device.class();
        if class != PCIDeviceClass::MassStorageCtrl || subclass != SATA_PCI_SUBCLASS {
            return Err(AHCIError::WrongPCIDevice);
        }

        let bmr_pa = pci_device.read_config(PCI_BAR_5);
        let bmr_ptr: *mut AHCIBaseMemoryReg =
            resolve_phys_addr(PhysAddr::new(bmr_pa as u64))
            .unwrap()
            .as_mut_ptr();
        let bmr = unsafe { bmr_ptr.as_mut().unwrap() };

        if !bmr.supports_64bit_addr() {
            return Err(AHCIError::DoesntSupport64BitAddr);
        }

        bmr.set_interrupts(true);

        let good_ports: Box<[usize]> = bmr.port_implemented.view_bits::<Lsb0>()
            .iter_ones()
            .collect();

        for p_i in &good_ports {
            let port = &bmr.ports[*p_i];
            println!("{:#?}", port);
            println!("{:#?}", port.command_list(bmr));
        }

        Ok(Self {
            base_mem_register: bmr,
            good_ports,
        })
    }
}

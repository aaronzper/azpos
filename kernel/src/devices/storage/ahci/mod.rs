use core::cmp::min;
use alloc::{boxed::Box, slice};
use error::AHCIError;
use mmio::{AHCIBaseMemoryReg, AHCICommandTable};
use bitvec::{order::Lsb0, view::BitView};
use x86_64::PhysAddr;

use crate::{devices::pci::{PCIDevice, PCIDeviceClass}, memory::{mmio::alloc_mmio_block, resolve_phys_addr}};

/// AHCI Memory-Mapped IO structures
mod mmio;
/// AHCI device types
mod types;
/// AHCI errors
mod error;
/// SATA Frame Information Structure stuff
mod fis;

pub const SATA_PCI_SUBCLASS: u8 = 0x6;
const PCI_BAR_5: u8 = 0x9;

/// How many command tables to allocate per port. The driver will allocate
/// either this many or however many the HBA supports, whichever is less.
///
/// Cant be more than 32
const MAX_COMMANDS_PER_PORT: u8 = 8;
/// The number of PRDT entries per command. 
///
/// Cant be more than 0xFFFF
const PRDT_ENTRIES_PER_COMMAND: u16 = 16;

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

        bmr.set_interrupts(false);

        let good_ports: Box<[usize]> = bmr.port_implemented.view_bits::<Lsb0>()
            .iter_ones()
            .filter(|i| bmr.ports[*i].device_detected())
            .collect();

        let commands_per_port = 
            min(MAX_COMMANDS_PER_PORT, bmr.num_supported_commands());
        // The number of command table to allocate
        let n_command_tables = commands_per_port as usize * good_ports.len();
        let alloc_size = n_command_tables * size_of::<AHCICommandTable>();

        let tables_pa = unsafe {
            let (_, frames) = alloc_mmio_block::<AHCICommandTable>(alloc_size)
                .expect("Couldn't allocate space for AHCI command tables");

            frames.start.start_address()
        };

        for (good_ports_index, bmr_index) in good_ports.iter().enumerate() {
            const CT_SIZE: u64 = size_of::<AHCICommandTable>() as u64;
        
            let port = &bmr.ports[*bmr_index];
            println!("Detected a {:?} AHCI drive", port.signature);

            let commands = port.command_list(bmr);
            let port_cts_index = commands_per_port as usize * good_ports_index;
            let port_cts_addr = tables_pa + (port_cts_index as u64 * CT_SIZE);
            for ct_i in 0..commands_per_port {
                let ct_addr = port_cts_addr + (ct_i as u64 * CT_SIZE);
                commands[ct_i as usize].command_table_addr = ct_addr;
            }
        }

        bmr.set_interrupts(true);
        Ok(Self {
            base_mem_register: bmr,
            good_ports,
        })
    }
}

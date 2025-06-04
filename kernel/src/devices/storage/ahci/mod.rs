use core::cmp::min;
use alloc::{boxed::Box, slice, vec::Vec};
use ata::{ATACommand, ATADriveInfo};
use device::AHCIDevice;
use error::AHCIError;
use fis::FISRegisterH2D;
use mmio::{AHCIBaseMemoryReg, AHCICommandTable, AHCIPort};
use bitvec::{order::Lsb0, view::BitView};
use types::AHCIDeviceType;
use x86_64::PhysAddr;

use crate::{devices::pci::{PCIDevice, PCIDeviceClass}, memory::{dealloc_frame, mmio::alloc_mmio_block, resolve_phys_addr}};

/// An individual AHCI device
mod device;
/// AHCI Memory-Mapped IO structures. Refer to the AHCI spec for more info.
mod mmio;
/// AHCI device types
mod types;
/// AHCI errors
mod error;
/// SATA Frame Information Structure stuff
mod fis;
/// ATA commands + structures
mod ata;

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
    devices: Box<[AHCIDevice]>,
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

        let mut devices = Vec::new();
        for (good_ports_index, bmr_index) in good_ports.iter().enumerate() {
            const CT_SIZE: u64 = size_of::<AHCICommandTable>() as u64;

            let port = unsafe { 
                // "Split off" the borrow:
                // This is fine since each port will only be split off like this
                // once, and they wont overlap with each other
                let ptr = &mut bmr.ports[*bmr_index] as *mut AHCIPort;
                ptr.as_mut().unwrap()
            };

            port.stop();

            // We only support SATA for now (who's using CD-ROMs in 2025 amirite)
            if port.signature == AHCIDeviceType::SATAPI { continue; }

            let commands = port.command_list(commands_per_port as usize);
            let port_cts_index = commands_per_port as usize * good_ports_index;
            let port_cts_addr = tables_pa + (port_cts_index as u64 * CT_SIZE);
            for ct_i in 0..commands_per_port {
                let ct_addr = port_cts_addr + (ct_i as u64 * CT_SIZE);
                commands[ct_i as usize].command_table_addr = ct_addr;
                commands[ct_i as usize].prdt_entries = PRDT_ENTRIES_PER_COMMAND;
                commands[ct_i as usize].flags.set_command_fis_len(
                    (size_of::<FISRegisterH2D>() / size_of::<u32>()) as u8
                );
            }
            
            let device = AHCIDevice::new(port, commands_per_port as usize);
            devices.push(device);
        }

        bmr.set_interrupts(true);
        Ok(Self {
            base_mem_register: bmr,
            devices: devices.into(),
        })
    }
}

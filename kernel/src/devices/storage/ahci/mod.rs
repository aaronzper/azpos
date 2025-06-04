use core::cmp::min;
use alloc::{boxed::Box, slice, string::String, vec::Vec};
use ata::{ATACommand, ATADriveInfo};
use error::AHCIError;
use fis::FISRegisterH2D;
use mmio::{AHCIBaseMemoryReg, AHCICommandTable};
use bitvec::{order::Lsb0, view::BitView};
use types::AHCIDeviceType;
use x86_64::PhysAddr;

use crate::{devices::pci::{PCIDevice, PCIDeviceClass}, memory::{dealloc_frame, mmio::alloc_mmio_block, resolve_phys_addr}};

/// AHCI Memory-Mapped IO structures
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

            let port = &mut bmr.ports[*bmr_index];
            port.stop();

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

            let bc = 512;

            commands[0].prdt_entries = 1;
            commands[0].command_table().command_fis = 
                FISRegisterH2D::new_with_type()
                .with_is_command(true)
                .with_command(ATACommand::IDENTIFY_DEVICE);

            let (buf_ptr, buf) = unsafe { alloc_mmio_block::<u16>(bc).unwrap() };

            commands[0].command_table().prdt[0].set_byte_count(bc as u32);
            commands[0].command_table().prdt[0].set_int_flag(true);
            commands[0].command_table().prdt[0].set_addr(buf_ptr);
            
            port.start();

            port.issue_commanmd(0);
            while port.command_busy(0) { }

            let data = unsafe { slice::from_raw_parts(buf_ptr, 256) };
            let info = ATADriveInfo::new(data.try_into().unwrap());
            let mib = (info.sectors() * info.sector_size()) as f32 / (1024.0 * 1024.0);
                
            println!("Detected a {:.2} MiB {:?} AHCI drive: {:?}", 
                mib, port.signature, info);

            for frame in buf {
                dealloc_frame(frame);
            }
        }

        bmr.set_interrupts(true);
        Ok(Self {
            base_mem_register: bmr,
            good_ports,
        })
    }
}

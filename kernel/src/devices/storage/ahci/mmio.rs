use modular_bitfield::prelude::*;
use bitvec::{order::Lsb0, view::BitView};
use x86_64::{PhysAddr, VirtAddr};
use crate::memory::{resolve_phys_addr, resolve_virt_addr, mmio::{read_bitfield, write_bitfield}};
use super::{fis::FISRegisterH2D, types::AHCIDeviceType, PRDT_ENTRIES_PER_COMMAND};
use core::slice;

#[repr(C)]
#[derive(Debug)]
pub struct AHCIBaseMemoryReg {
    pub host_capabilities: u32,
    pub global_host_control: u32,
    pub interrupt_status: u32,
    pub port_implemented: u32,
    pub version: u32,
    pub cmd_completion_coalescing_ctrl: u32,
    pub cmd_completion_coalescing_ports: u32,
    pub enclosure_mgmt_location: u32,
    pub enclosure_mgmt_ctrl: u32,
    pub host_capabilities_2: u32,
    pub bios_os_handoff_status: u32,

    reserved: [u8; 0x74],

    pub vendor_specific_regs: [u8; 0x60],

    pub ports: [AHCIPort; 32],
}

impl AHCIBaseMemoryReg {
    /// Does the HBA support 64-bit addressing?
    pub fn supports_64bit_addr(&self) -> bool {
        read_bitfield::<u32, u8>(self.host_capabilities, 31, 32) != 0
    }

    /// Enable or disable interrupts, HBA-wide
    pub fn set_interrupts(&mut self, ints: bool) {
        self.global_host_control.view_bits_mut::<Lsb0>().set(1, ints);
    }

    /// Returns the number of commands supported by the HBA
    pub fn num_supported_commands(&self) -> u8 {
        // Raw value "0" really means 1
        read_bitfield::<u32, u8>(self.host_capabilities, 8, 13) + 1
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct AHCIPort {
    pub cmd_list_base_addr: PhysAddr,
    pub fis_base_addr: PhysAddr,

    pub int_status: u32,
    pub int_enable: u32,
    pub command_status: u32,
    reserved_1: u32,
    pub task_file_data: u32,
    pub signature: AHCIDeviceType,
    pub sata_status: u32,
    pub sata_ctrl: u32,
    pub sata_error: u32,
    pub sata_active: u32,
    pub command_issue: u32,
    pub sata_notif: u32,
    pub fis_based_switch_ctrl: u32,
    reserved_2: [u32; 11],
    pub vendor_specific: [u32; 4],
}

impl AHCIPort {
    /// Returns a slice to the port's command list, given a reference to the HBA
    /// base memory register (needed to determine the size of the list)
    pub fn command_list(&self, n_commands: usize) -> &mut [AHCICommandHeader] {
        let va = resolve_phys_addr(self.cmd_list_base_addr)
            .expect("Command List unmapped!");
        let ptr = va.as_mut_ptr() as *mut AHCICommandHeader;
        unsafe { slice::from_raw_parts_mut(ptr, n_commands) }
    }

    pub fn fis(&self) -> &ReceivedFIS {
        let va = resolve_phys_addr(self.fis_base_addr)
            .expect("Received FIS unmapped!");
        let ptr = va.as_mut_ptr() as *const ReceivedFIS;
        unsafe { ptr.as_ref().unwrap() }
    }

    /// Returns true if sata_status.IPM is active and sata_status.DET is
    /// detected/connected
    pub fn device_detected(&self) -> bool {
        const IPM_ACTIVE: u8 = 0x1;
        const DET_ACTIVE: u8 = 0x3;
    
        let ipm: u8 = read_bitfield(self.sata_status, 8, 12);
        let det: u8 = read_bitfield(self.sata_status, 0, 4);

        ipm == IPM_ACTIVE && det == DET_ACTIVE
    }

    /// Sets the port to start receiving commands!
    pub fn start(&mut self) {
        // Make sure Px.CMD_CR is clear (not currently processing a command)
        while read_bitfield::<u32, u8>(self.command_status, 15, 16) != 0 {}

        // PxCMD_ST
        write_bitfield(&mut self.command_status, 0, 1, 1u64);
        // PxCMD_FRE
        write_bitfield(&mut self.command_status, 4, 5, 1u64);
    }

    /// Sets the port to stop receiving commands!
    pub fn stop(&mut self) {
        // PxCMD_ST
        write_bitfield(&mut self.command_status, 0, 1, 0u64);
        // PxCMD_FRE
        write_bitfield(&mut self.command_status, 4, 5, 0u64);

        loop {
            let st: u8 = read_bitfield(self.command_status, 0, 1);
            if st != 0 { continue; }

            let fre: u8 = read_bitfield(self.command_status, 4, 5);
            if fre != 0 { continue; }

            break;
        }
    }

    /// Modifies the port's command issue field to start the command at the
    /// specified index
    pub fn issue_command(&mut self, command_index: usize) {
        write_bitfield(&mut self.command_issue, command_index, command_index + 1, 1u64);
    }

    /// Returns whether the command at the given index is currently being
    /// handled by the device
    pub fn command_busy(&self, command_index: usize) -> bool {
        read_bitfield::<u32, u8>(self.command_issue, command_index, command_index + 1)
            == 1
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct ReceivedFIS {
    pub dma_setup: [u8; 28],
    padding_0: [u8; 4],

    pub pio_setup: [u8; 20],
    padding_1: [u8; 12],

    pub register_d2h: [u8; 20],
    padding_2: [u8; 4],

    pub set_device_bits: [u8; 8],

    pub unknown_fis: [u8; 64],

    reserved: [u8; 0x60],
}

#[bitfield]
#[repr(C)]
#[derive(Debug)]
pub struct AHCICommandHeaderFlags {
    /// Length of the Command FIS in 32-bit dwords 
    pub command_fis_len: B5,
    pub atapi: bool,
    pub write: bool,
    pub prefetchable: bool,
    pub reset: bool,
    pub bist: bool,
    pub clear_busy_on_ok: bool,
    reserved: bool,
    pub port_mult_port: B4,
}

#[repr(C)]
#[derive(Debug)]
pub struct AHCICommandHeader {
    pub flags: AHCICommandHeaderFlags,

    /// Number of Physical Regional Descriptor Table entries
    pub prdt_entries: u16,
    /// Physical regional descriptor byte count transferred
    pub prd_bytes_trans: u32,
    
    pub command_table_addr: PhysAddr,

    reserved: [u32; 4],
}

impl AHCICommandHeader {
    pub fn command_table(&self) -> &mut AHCICommandTable {
        let va = resolve_phys_addr(self.command_table_addr)
            .expect("Command Table unmapped!");
        let ptr = va.as_mut_ptr() as *mut AHCICommandTable;
        unsafe { ptr.as_mut().unwrap() }
    }
}

#[repr(C)]
#[repr(align(128))]
#[derive(Debug)]
pub struct AHCICommandTable {
    pub command_fis: FISRegisterH2D,
    /// Make Command FIS take up 64 bytes
    reserved_0: [u8; 64 - size_of::<FISRegisterH2D>()],

    /// Could also be 12 bytes, 13-16 are 0(?)
    pub atapi_command: [u8; 16], 

    reserved_1: [u8; 48],

    /// Up to 0xFFFF of these
    pub prdt: [PRDTEntry; PRDT_ENTRIES_PER_COMMAND as usize],
}

#[repr(C)]
#[derive(Debug)]
pub struct PRDTEntry {
    data_base_address: PhysAddr, 
    reserved_1: u32,
    
    /// Max u22, bit 0 is always 1, bits 22-30 are reserved, bit 31 is interrupt
    /// on complete flag
    byte_count: u32,
}

impl PRDTEntry {    
    /// The maximum size, in bytes, of a single PRDT entry's data. Set by AHCI
    /// spec to be 4MiB.
    pub const MAX_DATA_SIZE: u32 = 0x400000;

    pub fn get_byte_count(&self) -> u32 {
        let raw_count = self.byte_count & ((1 << 22) - 1);

        // Per spec (see comment to other function), byte count '1' actually
        // means 2 bytes. God help me
        raw_count + 1 
    }

    /// *See page 43 of the Intel AHCI 1.3.1 spec for the absolute bullshit going
    /// on in here*
    ///
    /// Parameters (panics if not met):
    /// - Byte count must not be zero
    /// - Byte count must be even
    /// - Byte count must be at most 4MiB (0x400000)
    pub fn set_byte_count(&mut self, count: u32) {
        if count > Self::MAX_DATA_SIZE {
            panic!("Byte count must be 22-bit");
        }

        if count == 0 {
            panic!("Byte count cannot be 0");
        }

        if count % 2 != 0 {
            panic!("Byte count must be even");
        }

        let upper = read_bitfield::<u32, u32>(self.byte_count, 22, 32) << 22;

        // The count value is 1 less than the actual number of bytes, see spec
        self.byte_count = upper | (count - 1); 
    }

    pub fn set_int_flag(&mut self, int: bool) {
        self.byte_count =
            ((int as u32) << 31)
            | (self.byte_count & ((1 << 31) - 1));
    }

    pub fn set_addr<T>(&mut self, data: *mut T) {
        let pa = resolve_virt_addr(VirtAddr::from_ptr(data)).unwrap();
        
        if (pa.as_u64() & 0b1) != 0 {
            panic!("PRDT address must be word (2-byte) aligned");
        }

        self.data_base_address = pa;
    }
}

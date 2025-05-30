use alloc::slice;
use x86_64::PhysAddr;

use crate::memory::resolve_phys_addr;

use super::types::AHCIDeviceType;

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
    pub fn command_list(&self, n_commands: u8) -> &mut [AHCICommandHeader] {
        let va = resolve_phys_addr(self.cmd_list_base_addr)
            .expect("Command List unmapped!");
        let ptr = va.as_mut_ptr() as *mut AHCICommandHeader;
        unsafe { slice::from_raw_parts_mut(ptr, n_commands as usize) }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct AHCICommandHeader {
    pub flags: u16,

    pub prdt_len: u16, // Phys regional desc table len in entries,
    pub prd_bytes_trans: u32, // PRD byte count transferred
    
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
pub struct AHCICommandTable {
    pub fis: [u8; 64],

    /// Could also be 12 bytes, 13-16 are 0(?)
    pub atapi_command: u16, 

    reserved: [u8; 48],

    prdt: [PRDTEntry; 0xFFFF], // May not be this many
}

impl AHCICommandTable {
    pub fn prdt(&mut self, n_entries: usize) -> &mut [PRDTEntry]  {
        let ptr = &raw mut self.prdt[0];
        unsafe { slice::from_raw_parts_mut(ptr, n_entries as usize) }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct PRDTEntry {
    pub data_base_address: PhysAddr, 
    reserved_1: u32,
    
    /// Max u22, bit 0 is always 1, bits 22-30 are reserved, bit 31 is interrupt
    /// on complete flag
    byte_count: u32,
}

impl PRDTEntry {    
    pub fn get_byte_count(&self) -> u32 {
        self.byte_count & ((1 << 22) - 1)
    }

    pub fn set_byte_count(&mut self, count: u32) {
        if count >= 1 << 22 {
            panic!("Byte count must be 22-bit");
        }
        
        let upper = self.byte_count & (((1 << 10) - 1) << 22);
        self.byte_count = upper | count;
    }

    pub fn set_int_flag(&mut self, int: bool) {
        self.byte_count =
            ((int as u32) << 31)
            | (self.byte_count & ((1 << 31) - 1));
    }
}

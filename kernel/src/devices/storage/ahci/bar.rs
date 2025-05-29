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
    pub cmd_list_base_addr: u64,
    pub fis_base_addr: u64,

    pub int_status: u32,
    pub int_enable: u32,
    pub command_status: u32,
    reserved_1: u32,
    pub task_file_data: u32,
    pub signature: u32,
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

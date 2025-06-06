use core::mem;
use modular_bitfield::prelude::*;

pub const MBR_SIGNATURE: u16 = 0xAA55;

#[derive(Debug)]
pub enum MBRPartitionType {
    Unknown,
    Empty,
    Fat32Lba,
    GPT,
}

#[bitfield]
#[repr(C)]
#[derive(Debug)]
pub struct MBRPartitionEntry {
    status: u8,
    pub chs_first: B24,
    raw_type: u8,
    pub chs_last: B24,
    pub lba_start: u32,
    pub num_sectors: u32,
}

impl MBRPartitionEntry {
    pub fn is_active(&self) -> bool {
        // Code for active/bootable. Any other code is inactive/invalid
        self.status() == 0x80
    }

    pub fn partition_type(&self) -> MBRPartitionType {
        match self.raw_type() {
            0x00 => MBRPartitionType::Empty,
            0x0C => MBRPartitionType::Fat32Lba,
            0xEE => MBRPartitionType::GPT,
            _ => MBRPartitionType::Unknown,
        }
    }
}

#[repr(C)]
#[derive(Debug)]
/// Partition entries of an MBR 
pub struct MasterBootRecord {
    pub entries: [MBRPartitionEntry; 4],
}

impl MasterBootRecord {
    /// Constructs an MBR from the bytes of a boot sector. Returns `None` if its
    /// invalid
    pub fn new(bytes: &[u8; 512]) -> Option<Self> {
        let signature = 
            u16::from_le_bytes(bytes[0x1FE..0x200].try_into().unwrap());

        if signature != MBR_SIGNATURE {
            return None;
        }

        let entries: [u8; 16*4] = bytes[0x1BE..0x1FE].try_into().unwrap();
        Some(unsafe { mem::transmute_copy(&entries) })
    }

    /// Returns an iterator to only the active partitions on the MBR
    pub fn active_partitions(&self) -> impl Iterator<Item = &MBRPartitionEntry> {
        self.entries.iter().filter(|e| e.is_active())
    }
}

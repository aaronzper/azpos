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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktest::*;

    /// Builds a boot sector with a signature. Each tuple is
    /// (slot, status, type, first sector, sector count).
    fn sector(entries: &[(usize, u8, u8, u32, u32)]) -> [u8; 512] {
        let mut s = [0u8; 512];
        for (slot, status, raw_type, first, count) in entries {
            let e = 0x1BE + slot * 16;
            s[e] = *status;
            s[e + 4] = *raw_type;
            s[e + 8..e + 12].copy_from_slice(&first.to_le_bytes());
            s[e + 12..e + 16].copy_from_slice(&count.to_le_bytes());
        }
        s[0x1FE..0x200].copy_from_slice(&MBR_SIGNATURE.to_le_bytes());
        s
    }

    ktest! {
        fn new_rejects_a_sector_with_no_signature() {
            let mut s = sector(&[]);
            check!(MasterBootRecord::new(&s).is_some());
            s[0x1FE] = 0;
            check!(MasterBootRecord::new(&s).is_none());
            check!(MasterBootRecord::new(&[0u8; 512]).is_none());
        }

        fn an_entry_decodes_each_field_as_little_endian() {
            let mbr = MasterBootRecord::new(&sector(&[(2, 0x80, 0x0C, 0x0102_0304, 0x0A0B_0C0D)]))
                .or_fail("new")?;
            let e = &mbr.entries[2];
            check_eq!(e.status(), 0x80);
            check_eq!(e.raw_type(), 0x0C);
            check_eq!(e.lba_start(), 0x0102_0304);
            check_eq!(e.num_sectors(), 0x0A0B_0C0D);
        }

        fn partition_type_decodes_each_known_type() {
            let mbr = MasterBootRecord::new(&sector(&[
                (0, 0, 0x00, 0, 0), (1, 0, 0x0C, 0, 0), (2, 0, 0xEE, 0, 0), (3, 0, 0x83, 0, 0),
            ])).or_fail("new")?;
            check_matches!(mbr.entries[0].partition_type(), MBRPartitionType::Empty);
            check_matches!(mbr.entries[1].partition_type(), MBRPartitionType::Fat32Lba);
            check_matches!(mbr.entries[2].partition_type(), MBRPartitionType::GPT);
            check_matches!(mbr.entries[3].partition_type(), MBRPartitionType::Unknown);
        }

        /// Only the status 0x80 is active.
        fn active_partitions_gives_only_status_0x80() {
            let mbr = MasterBootRecord::new(&sector(&[
                (0, 0x80, 0x0C, 10, 1), (1, 0x00, 0x0C, 20, 1), (2, 0x81, 0x0C, 30, 1), (3, 0x80, 0x0C, 40, 1),
            ])).or_fail("new")?;
            let starts: alloc::vec::Vec<u32> = mbr.active_partitions().map(|e| e.lba_start()).collect();
            check_eq!(starts, [10, 40]);
        }

        fn a_partition_entry_is_16_bytes() {
            check_eq!(mem::size_of::<MBRPartitionEntry>(), 16);
            check_eq!(mem::size_of::<MasterBootRecord>(), 64);
        }
    }
}

use alloc::{boxed::Box, vec::Vec};
use super::{boot_record::FATBootRecord, FATType};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum FATEntry {
    Free,
    Allocated { next: u32 },
    AllocatedEOF,
    Bad,
    Unknown,
}

pub struct FileAllocationTable {
    /// The actual bytes underlying the FAT
    raw_data: Box<[u8]>,
    /// The type of FAT this is
    fat_type: FATType,
    /// The number of valid clusters
    num_clusters: u32,
}

impl FileAllocationTable {
    /// Creates a new File Allocation Table from
    ///
    /// Returns `None` if the given data isn't of the right format per the
    /// boot record
    pub fn new(raw_data: Box<[u8]>, boot_record: &FATBootRecord) -> Option<Self> {
        let sectors = raw_data.len() / boot_record.bytes_per_sector as usize;

        if sectors != boot_record.fat_size() as usize {
            return None;
        }
        
        Some(Self {
            raw_data,
            fat_type: boot_record.fat_type(),
            num_clusters: boot_record.cluster_count(),
        })
    }

    pub fn iter<'a>(&'a self) -> FATIterator<'a> {
        FATIterator {
            fat: self,
            index: 0,
        }
    }

    /// Traverses the FAT to get a cluster chain sarting at a certain index.
    ///
    /// If the cluster at `start_index` is part of a valid chain, returns a
    /// list of clusters indexes making up the chain, starting with `start_index`.
    ///
    /// If it isn't (references a free cluster, an invalid index, etc),
    /// returns `None`.
    pub fn get_chain(&self, start_index: u32) -> Option<Box<[u32]>> {
        let mut chain = Vec::new();
        let mut i = start_index;
        loop {
            match self.get_entry(i)? {
                FATEntry::Allocated { next } => {
                    chain.push(i);
                    i = next;
                },

                FATEntry::AllocatedEOF => {
                    chain.push(i);
                    break;
                },

                _ => return None,
            }
        }

        Some(chain.into())
    }

    pub fn get_entry(&self, index: u32) -> Option<FATEntry> {
        if index >= self.num_clusters {
            return None;
        }

        // First 2 FAT entries are reserved
        if index < 2 {
            return Some(FATEntry::Unknown);
        }

        match self.fat_type {
            FATType::Fat32 => {
                const ENTRY_LEN: usize = 4;

                let offset = index as usize * ENTRY_LEN;
                let entry_bytes: [u8; ENTRY_LEN] = 
                    self.raw_data[offset..(offset+ENTRY_LEN)].try_into().unwrap();

                // Mask out the top 4 bits, so 28-bit value
                let entry_raw = u32::from_le_bytes(entry_bytes) & 0x0FFFFFFF; 

                Some(match entry_raw {
                    0x0 => FATEntry::Free,

                    next if (0x2..self.num_clusters).contains(&next) => 
                        FATEntry::Allocated { next },

                    0xFFFFFF7  => FATEntry::Bad,
                    0xFFFFFF8.. => FATEntry::AllocatedEOF,

                    _ => FATEntry::Unknown,
                })
            },

            _ => unimplemented!("12-/16-bit FATs")
        }
    }
}

pub struct FATIterator<'a> {
    fat: &'a FileAllocationTable,
    index: u32,
}

impl Iterator for FATIterator<'_> {
    type Item = FATEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.fat.get_entry(self.index);
        self.index += 1;
        item
    }
}

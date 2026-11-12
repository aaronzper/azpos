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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::boot_record::tests::Geometry;
    use crate::ktest::*;

    /// The number of clusters of `geometry()`
    const COUNT: u32 = 16;
    const BAD: u32 = 0x0FFF_FFF7;
    const EOF: u32 = 0x0FFF_FFFF;

    /// A FAT32 volume with one FAT of one sector and 16 clusters
    fn geometry() -> Geometry {
        Geometry { num_fats: 1, fat_size_32: 1, total_32: 32 + 1 + COUNT, ..Geometry::FAT32 }
    }

    /// Builds a table of one sector. Each pair is (index, raw entry).
    fn table(entries: &[(u32, u32)]) -> FileAllocationTable {
        let mut raw = [0u8; 512];
        raw[0..4].copy_from_slice(&0x0FFF_FFF8u32.to_le_bytes());
        raw[4..8].copy_from_slice(&EOF.to_le_bytes());
        for (index, value) in entries {
            let offset = *index as usize * 4;
            raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        FileAllocationTable::new(Box::from(raw), &geometry().record()).unwrap()
    }

    ktest! {
        fn new_rejects_data_that_is_not_the_fat_size() {
            let br = geometry().record();
            check!(FileAllocationTable::new(Box::from([0u8; 1024]), &br).is_none());
            check!(FileAllocationTable::new(Box::from([0u8; 0]), &br).is_none());
            check!(FileAllocationTable::new(Box::from([0u8; 512]), &br).is_some());
        }

        fn entries_0_and_1_are_unknown() {
            let fat = table(&[]);
            check_eq!(fat.get_entry(0), Some(FATEntry::Unknown));
            check_eq!(fat.get_entry(1), Some(FATEntry::Unknown));
        }

        fn get_entry_decodes_each_kind() {
            let fat = table(&[(2, 0), (3, 5), (4, BAD), (5, 0x0FFF_FFF8), (6, EOF)]);
            check_eq!(fat.get_entry(2), Some(FATEntry::Free));
            check_eq!(fat.get_entry(3), Some(FATEntry::Allocated { next: 5 }));
            check_eq!(fat.get_entry(4), Some(FATEntry::Bad));
            check_eq!(fat.get_entry(5), Some(FATEntry::AllocatedEOF));
            check_eq!(fat.get_entry(6), Some(FATEntry::AllocatedEOF));
        }

        fn get_entry_ignores_the_top_4_bits() {
            let fat = table(&[(2, 0xF000_0003), (3, 0xF000_0000)]);
            check_eq!(fat.get_entry(2), Some(FATEntry::Allocated { next: 3 }));
            check_eq!(fat.get_entry(3), Some(FATEntry::Free));
        }

        /// A next cluster of 1 is not a valid link.
        /// A value between the clusters and the bad marker is not a valid link.
        fn get_entry_of_a_reserved_value_is_unknown() {
            let fat = table(&[(2, 1), (3, 1000), (4, 0x0FFF_FFF0)]);
            check_eq!(fat.get_entry(2), Some(FATEntry::Unknown));
            check_eq!(fat.get_entry(3), Some(FATEntry::Unknown));
            check_eq!(fat.get_entry(4), Some(FATEntry::Unknown));
        }

        fn get_chain_of_a_one_cluster_file() {
            let fat = table(&[(2, EOF)]);
            check_eq!(fat.get_chain(2).as_deref(), Some(&[2][..]));
        }

        fn get_chain_follows_links_out_of_order() {
            let fat = table(&[(2, 9), (9, 4), (4, EOF)]);
            check_eq!(fat.get_chain(2).as_deref(), Some(&[2, 9, 4][..]));
        }

        fn get_chain_of_an_invalid_start_gives_none() {
            let fat = table(&[(2, EOF)]);
            check!(fat.get_chain(0).is_none());
            check!(fat.get_chain(1).is_none());
            check!(fat.get_chain(3).is_none(), "start at a free cluster");
            check!(fat.get_chain(100).is_none(), "start past the table");
            check!(fat.get_chain(u32::MAX).is_none());
        }

        fn get_chain_that_reaches_a_free_or_bad_cluster_gives_none() {
            let fat = table(&[(2, 3), (3, 0), (4, 5), (5, BAD)]);
            check!(fat.get_chain(2).is_none());
            check!(fat.get_chain(4).is_none());
        }

        /// The clusters are 2 to `count + 1`, so `count` and `count + 1` are valid.
        /// Today `get_entry` rejects an index of `count` or more.
        #[known_bug(unfiled)]
        fn the_last_two_clusters_are_valid() {
            let fat = table(&[(2, COUNT + 1), (COUNT, EOF), (COUNT + 1, COUNT)]);
            check_eq!(fat.get_entry(2), Some(FATEntry::Allocated { next: COUNT + 1 }));
            check_eq!(fat.get_entry(COUNT + 1), Some(FATEntry::Allocated { next: COUNT }));
            check_eq!(fat.get_entry(COUNT), Some(FATEntry::AllocatedEOF));
            check_eq!(fat.get_entry(COUNT + 2), None);
        }

        /// A chain with a cycle is corrupt, so `get_chain` gives `None`.
        /// Today `get_chain` loops and fills the heap.
        #[known_bug(unfiled, crash)]
        #[timeout(5)]
        fn get_chain_with_a_cycle_gives_none() {
            let fat = table(&[(2, 3), (3, 2)]);
            check!(fat.get_chain(2).is_none());
        }
    }
}

use core::{ascii, ops::Index};
use alloc::{boxed::Box, format, slice, string::String};
use modular_bitfield::{error::OutOfBounds, prelude::*};

#[bitfield]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FATFileAttributes {
    pub read_only: bool,
    pub hidden: bool,
    pub system: bool,
    pub volume_id: bool,
    pub directory: bool,
    pub archive: bool,
    reserved: B2,
}

impl FATFileAttributes {
    pub fn long_file_name(&self) -> bool {
        self.read_only() && self.hidden() && self.system() && self.volume_id()
    }
}

#[bitfield]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FATDate {
    pub day: B5,
    pub month: B4,
    /// `0` is 1980
    year_raw: B7,
}

impl FATDate {
    const YEAR_OFFSET: u32 = 1980;
    
    pub fn year(&self) -> u32 {
        self.year_raw() as u32 + Self::YEAR_OFFSET
    }

    pub fn set_seconds(&mut self, year: u32) -> Result<(), OutOfBounds> {
        let raw = year - Self::YEAR_OFFSET;
        self.set_year_raw_checked(raw as u8)
    }
}

#[bitfield]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FATTime {
    /// Counts in 2-second increments
    seconds_half: B5,
    pub minutes: B6,
    pub hours: B5,
}

impl FATTime {
    pub fn seconds(&self) -> u8 {
        self.seconds_half() * 2
    }

    pub fn set_seconds(&mut self, seconds: u8) -> Result<(), OutOfBounds> {
        self.set_seconds_half_checked(seconds / 2)
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct FATDirectoryEntry {
    pub file_name: [ascii::Char; 8],
    pub file_extension: [ascii::Char; 3],
    pub attributes: FATFileAttributes,
    reserved: u8,
    pub creation_time_tenth: u8,
    pub creation_time: FATTime,
    pub creation_date: FATDate,
    pub last_accessed_date: FATDate,
    first_cluster_high: u16,
    pub write_time: FATTime,
    pub write_date: FATDate,
    first_cluster_low: u16,
    pub file_size: u32,
}

impl FATDirectoryEntry {
    /// If the given entry is free to be used
    pub fn is_free(&self) -> bool {
        // Per spec, if first byte of an entry is 0xE5, its free, but NOT last
        self.file_name.as_bytes()[0] == 0xE5 || self.is_last()
    }

    /// Per spec, the entry is free AND all entries after are free if the first
    /// byte is 0x00
    fn is_last(&self) -> bool {
        self.file_name.as_bytes()[0] == 0x00
    }

    /// Returns the full name, including extension, of the file. Returns `None`
    /// if the entry is free or an LFN entry
    pub fn full_name(&self) -> Option<String> {
        if self.is_free() || self.attributes.long_file_name() {
            return None;
        }

        let name = self.file_name.as_str().trim_end();
        let ext = self.file_extension.as_str().trim_end();

        Some(if ext.is_empty() {
            String::from(name)
        } else {
            format!("{name}.{ext}")
        })
    }

    /// Returns the cluster number of the file/dir pointed to by the entry
    pub fn cluster(&self) -> u32 {
        self.first_cluster_low as u32 | ((self.first_cluster_high as u32) << 16)
    }
}

#[derive(Debug)]
pub struct FATDirectory {
    entries: Box<[FATDirectoryEntry]>,
    len: usize,
}

impl FATDirectory {
    /// Returns `None` if the given raw data's size isnt a multiple of 32
    pub fn new(data: Box<[u8]>) -> Option<Self> {
        if data.len() % 32 != 0 {
            return None;
        }
        let len = data.len() / 32;

        let ptr_raw: *mut [u8] = Box::into_raw(data);
        let ptr = ptr_raw as *mut FATDirectoryEntry;

        let entries = unsafe { 
            let slice = slice::from_raw_parts_mut(ptr, len);
            Box::from_raw(slice)
        };

        let len = match entries.iter().enumerate().find(|(_, e)| e.is_last()) {
            // We found empty, so that index is the length
            Some((i, _)) => i,
            // None were empty, so full length
            None => entries.len(),
        };

        Some(Self { entries, len })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn iter<'a>(&'a self) -> FATDirIter<'a> {
        FATDirIter {
            dir: self,
            index: 0,
        }
    }

    /// Finds and returns a directory entry by filename, if it exists
    pub fn find(&self, filename: &str) -> Option<&FATDirectoryEntry> {
        self.iter().find(|e| {
            if let Some(s) = e.full_name() {
                s.as_str() == filename
            } else {
                false
            }
        })
    }
}

impl Index<usize> for FATDirectory {
    type Output = FATDirectoryEntry;

    fn index(&self, index: usize) -> &Self::Output {
        if index >= self.len() {
            panic!("Directory index {} is out of bounds ({} entries)",
                index, self.len());
        }

        &self.entries[index]
    }
}

pub struct FATDirIter<'a> {
    dir: &'a FATDirectory,
    index: usize,
}

impl<'a> Iterator for FATDirIter<'a> {
    type Item = &'a FATDirectoryEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.dir.len() {
            return None;
        }

        let e = &self.dir[self.index];
        self.index += 1;
        Some(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktest::*;
    use alloc::vec::Vec;

    /// Builds one raw 32-byte directory entry with an 8.3 name
    fn raw_entry(name: &[u8; 8], ext: &[u8; 3], attributes: u8, cluster: u32, size: u32) -> [u8; 32] {
        let mut raw = [0u8; 32];
        raw[0..8].copy_from_slice(name);
        raw[8..11].copy_from_slice(ext);
        raw[11] = attributes;
        raw[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
        raw[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
        raw[28..32].copy_from_slice(&size.to_le_bytes());
        raw
    }

    const ATTR_DIRECTORY: u8 = 0x10;
    const ATTR_LONG_NAME: u8 = 0x0F;

    /// A directory with a file, a long-name entry, a subdirectory, then the
    /// end marker and one entry after it
    fn sample_dir() -> FATDirectory {
        let mut data = Vec::new();
        data.extend_from_slice(&raw_entry(b"HELLO   ", b"TXT", 0, 0x0012_0034, 26));
        data.extend_from_slice(&raw_entry(b"Aname   ", b"   ", ATTR_LONG_NAME, 0, 0));
        data.extend_from_slice(&raw_entry(b"NESTED  ", b"   ", ATTR_DIRECTORY, 9, 0));
        data.extend_from_slice(&[0u8; 32]);
        data.extend_from_slice(&raw_entry(b"AFTER   ", b"END", 0, 5, 1));
        FATDirectory::new(data.into()).unwrap()
    }

    ktest! {
        fn new_rejects_a_size_that_is_not_a_multiple_of_32() {
            check!(FATDirectory::new(Box::from([0u8; 33])).is_none());
            check!(FATDirectory::new(Box::from([0u8; 64])).is_some());
        }

        fn the_end_marker_sets_the_length() {
            check_eq!(sample_dir().len(), 3);
            check_eq!(sample_dir().iter().count(), 3);
        }

        fn full_name_joins_name_and_extension() {
            let dir = sample_dir();
            check_eq!(dir[0].full_name().as_deref(), Some("HELLO.TXT"));
            check_eq!(dir[2].full_name().as_deref(), Some("NESTED"));
        }

        fn a_long_name_entry_has_no_full_name() {
            check_eq!(sample_dir()[1].full_name(), None);
        }

        fn find_gives_the_entry_with_cluster_and_size() {
            let dir = sample_dir();
            let entry = dir.find("HELLO.TXT").or_fail("find")?;
            check_eq!(entry.cluster(), 0x0012_0034);
            check_eq!({ entry.file_size }, 26);
            check!(!entry.attributes.directory());

            check!(dir.find("NESTED").or_fail("find")?.attributes.directory());
            check!(dir.find("MISSING.TXT").is_none());
            check!(dir.find("AFTER.END").is_none());
        }

        /// The first byte 0xE5 marks a free entry.
        /// A free entry does not end the directory.
        fn a_free_entry_is_skipped_but_does_not_end_the_directory() {
            let mut data = Vec::new();
            data.extend_from_slice(&raw_entry(b"FIRST   ", b"TXT", 0, 3, 1));
            data.extend_from_slice(&raw_entry(b"\xE5ELETED ", b"TXT", 0, 4, 1));
            data.extend_from_slice(&raw_entry(b"LAST    ", b"TXT", 0, 5, 1));
            let dir = FATDirectory::new(data.into()).or_fail("new")?;

            check_eq!(dir.len(), 3);
            check!(dir[1].is_free());
            check_eq!(dir[1].full_name(), None);
            check_eq!(dir.iter().filter_map(|e| e.full_name()).count(), 2);
            check_eq!(dir.find("LAST.TXT").or_fail("find")?.cluster(), 5);
        }

        fn long_file_name_needs_all_four_attribute_bits() {
            check!(FATFileAttributes::from_bytes([0x0F]).long_file_name());
            check!(FATFileAttributes::from_bytes([0x3F]).long_file_name());
            for raw in [0x00, 0x07, 0x0B, 0x0D, 0x0E, 0x10] {
                check!(!FATFileAttributes::from_bytes([raw]).long_file_name(), "raw {:#x}", raw);
            }
        }

        fn fat_date_decodes_day_month_and_year() {
            // Day bits 0-4, month bits 5-8, year bits 9-15
            let raw: u16 = 15 | (6 << 5) | (44 << 9);
            let date = FATDate::from_bytes(raw.to_le_bytes());
            check_eq!(date.day(), 15);
            check_eq!(date.month(), 6);
            check_eq!(date.year(), 2024);
        }

        fn fat_time_decodes_seconds_minutes_and_hours() {
            // 2-second count bits 0-4, minutes bits 5-10, hours bits 11-15
            let raw: u16 = 10 | (34 << 5) | (13 << 11);
            let time = FATTime::from_bytes(raw.to_le_bytes());
            check_eq!(time.seconds(), 20);
            check_eq!(time.minutes(), 34);
            check_eq!(time.hours(), 13);
        }

        fn fat_time_set_seconds_rounds_down_and_rejects_too_large() {
            let mut time = FATTime::new();
            check!(time.set_seconds(21).is_ok());
            check_eq!(time.seconds(), 20);
            check!(time.set_seconds(64).is_err());
        }

        /// `FATDate::set_seconds` sets the year.
        fn fat_date_set_seconds_sets_the_year() {
            let mut date = FATDate::new();
            check!(date.set_seconds(2107).is_ok());
            check_eq!(date.year(), 2107);
            check!(date.set_seconds(2108).is_err());
        }

        /// A year of 2236 does not fit in 7 bits, so the set gives `Err`.
        /// Today the cast to `u8` drops the high bits, and the year becomes 1980.
        #[known_bug(unfiled)]
        fn fat_date_rejects_a_year_that_wraps_in_u8() {
            let mut date = FATDate::new();
            check!(date.set_seconds(2236).is_err());
            check_eq!(date.year(), 1980);
        }

        /// A year before 1980 gives `Err`.
        /// Today the subtraction overflows and the kernel panics.
        #[known_bug(unfiled, crash)]
        fn fat_date_rejects_a_year_before_1980() {
            let mut date = FATDate::new();
            check!(date.set_seconds(1979).is_err());
        }

        fn a_name_of_8_characters_with_no_extension_has_no_dot() {
            let data = raw_entry(b"ABCDEFGH", b"   ", 0, 3, 1);
            let dir = FATDirectory::new(Box::from(data)).or_fail("new")?;
            check_eq!(dir[0].full_name().as_deref(), Some("ABCDEFGH"));
        }

        fn an_empty_directory_has_no_entries() {
            let dir = FATDirectory::new(Box::from([0u8; 64])).or_fail("new")?;
            check_eq!(dir.len(), 0);
            check!(dir.find("").is_none());
        }

        /// An index past the last entry is a kernel bug, so the kernel panics.
        #[should_panic("is out of bounds (3 entries)")]
        fn an_index_past_the_end_panics() {
            let dir = sample_dir();
            let _ = &dir[3];
        }
    }
}

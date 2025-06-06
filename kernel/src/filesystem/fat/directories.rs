use core::{ascii, ops::Index};
use alloc::{boxed::Box, slice};
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
#[derive(Debug)]
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
    /// Per spec, if first byte of an entry is 0xE5, its free, but NOT last
    pub fn is_free(&self) -> bool {
        self.file_name.as_bytes()[0] == 0xE5
    }

    /// Per spec, the entry is free AND all entries after are free if the first
    /// byte is 0x00
    fn is_last(&self) -> bool {
        self.file_name.as_bytes()[0] == 0x00
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

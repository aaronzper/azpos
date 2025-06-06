use core::ascii;
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

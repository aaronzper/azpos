use alloc::{string::{String, ToString}, vec::Vec};
use modular_bitfield::prelude::*;

use crate::memory::mmio::read_bitfield;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(Specifier)]
#[bits = 8]
pub enum ATACommand {
    NOP             = 0x00,
    READ_DMA_EXT    = 0x25,
    WRITE_DMA_EXT   = 0x35,
    IDENTIFY_DEVICE = 0xEC,
    
}

/// Data returned by the ATA `IDENTIFY DEVICE` command
#[derive(Debug, Clone)]
pub struct ATADriveInfo {
    model_name: String,
    num_sectors: usize,
    sector_size: usize,
}

impl ATADriveInfo {
    /// Constructs `Self` based off the raw data returned by `IDENTIFY DEVICE`
    pub fn new(identify_data: &[u16; 256]) -> Self {
        let model_name = identify_data[27..47].iter()
            .flat_map(|x| x.to_be_bytes())
            .map(|x| x as char)
            .collect::<String>()
            .trim_end()
            .to_string();

        let num_sectors: usize = u64::from_le_bytes(
            identify_data[100..104].iter()
            .flat_map(|x| x.to_le_bytes())
            .collect::<Vec<u8>>()
            .try_into().unwrap()
        ) as usize;

        let sector_size = {
            const DEFAULT_SIZE: usize = 512;

            let word = identify_data[106];
            if read_bitfield::<u16, u8>(word, 14, 16) == 0b01 &&
               read_bitfield::<u16, u8>(word, 12, 13) == 0b1 {
                u32::from_le_bytes(
                    identify_data[117..119].iter()
                    .flat_map(|x| x.to_le_bytes())
                    .collect::<Vec<u8>>()
                    .try_into().unwrap()
                ) as usize
            } else {
                DEFAULT_SIZE
            }
        };

        Self {
            model_name,
            num_sectors,
            sector_size,
        }
    }

    /// Returns the name of the drive
    pub fn name(&self) -> &str {
        &self.model_name
    }

    /// Returns the number of sectors on the drive
    pub fn sectors(&self) -> usize {
        self.num_sectors
    }

    /// Returns the size of each sector in bytes
    pub fn sector_size(&self) -> usize {
        self.sector_size
    }
}

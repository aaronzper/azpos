use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String};
use libsci::resources::{Resource, ResourceID};
use x86_64::structures::paging::page_table::PageTableEntry;

use crate::{memory::current_pt, utils::id_table::IDTable};

const USER_PT_LEN: usize = 256;

pub type ProcessID = u32;

/// An individual user process
pub struct Process {
    /// The process's name
    name: String,

    /// The lower (user) half of the L4PT for this process. This is copied into
    /// the lower half of the (universal) page table upon context switch. The
    /// upper half (kernel memory) doesn't change.
    user_page_table: [PageTableEntry; USER_PT_LEN],
    
    /// The Resources owned by the process
    pub resources: IDTable<ProcessID, Box<dyn Resource + Send>>,
}

impl Process {
    /// Creates a new process by parsing the given ELF data, loading it into
    /// memory, and creating a starting thread.
    ///
    /// Returns `None` if the ELF data is invalid
    pub fn new(name: String) -> Self {
        Self {
            name,
            user_page_table: [const { PageTableEntry::new() }; USER_PT_LEN],
            resources: IDTable::new(),
        }
    }

    /// Saves the user half of the current page table to the process. Used in
    /// context switching.
    pub fn save_page_tables(&mut self) {
        let pt = current_pt();
        for i in 0..USER_PT_LEN {
            self.user_page_table[i] = pt.level_4_table()[i].clone();
        }
    }

    /// Loads the user half of the current page table from the process. Used in
    /// context switching.
    pub fn load_page_tables(&self) {
        let mut pt = current_pt();
        for i in 0..USER_PT_LEN {
            pt.level_4_table_mut()[i] = self.user_page_table[i].clone();
        }
        x86_64::instructions::tlb::flush_all();
    }
}

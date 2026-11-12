use x86_64::structures::paging::{page::PageRangeInclusive, PageTableFlags};
use crate::processes::elfdefs::{ELF_PT_FLAG_R, ELF_PT_FLAG_W, ELF_PT_FLAG_X};
use super::PAGE_ALLOCATOR;

pub const USER_END_ADDR: u64 = 0x0000_8000_0000_0000;

#[derive(Debug, Clone, Copy)]
pub struct UserMemoryFlags {
    pub read: bool,
    pub write: bool,
    pub exec: bool,
}

impl UserMemoryFlags {
    /// Converts an ELF p_flags value into `Self`
    pub fn from_elf_flags(p_flags: u32) -> Self {
        UserMemoryFlags {
            read:  p_flags & ELF_PT_FLAG_R != 0,
            write: p_flags & ELF_PT_FLAG_W != 0,
            exec:  p_flags & ELF_PT_FLAG_X != 0,
        }
    }
}

/// Allocates physical space for the given pages in the currently loaded user
/// memory, with the given permission flags
pub fn alloc_user_pages(pages: PageRangeInclusive, flags: UserMemoryFlags) {
    let end_addr = pages.end.start_address();
    if end_addr.as_u64() >= USER_END_ADDR {
        panic!("Tried to allocate user memory in kernelspace");
    }

    let mut page_flags = PageTableFlags::empty() 
        | PageTableFlags::PRESENT 
        | PageTableFlags::USER_ACCESSIBLE;

    if flags.write {
        page_flags |= PageTableFlags::WRITABLE;
    }
    if !flags.exec {
        page_flags |= PageTableFlags::NO_EXECUTE;
    }
    
    let mut lock = PAGE_ALLOCATOR.lock();
    let page_alloc = lock.as_mut().unwrap();
    for page in pages {
        page_alloc.alloc_page(page, page_flags);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktest::*;

    ktest! {
        /// Bit 0 is exec, bit 1 is write and bit 2 is read (ELF `PF_X`, `PF_W`, `PF_R`).
        /// Today `elfdefs.rs` swaps the values of the read and write bits.
        #[known_bug(64)]
        fn from_elf_flags_reads_each_bit() {
            let x = UserMemoryFlags::from_elf_flags(0b001);
            check!(x.exec && !x.write && !x.read);
            let w = UserMemoryFlags::from_elf_flags(0b010);
            check!(!w.exec && w.write && !w.read);
            let r = UserMemoryFlags::from_elf_flags(0b100);
            check!(!r.exec && !r.write && r.read);
        }

        /// No bit gives no permission. All bits give all permissions.
        fn from_elf_flags_of_none_and_all() {
            let none = UserMemoryFlags::from_elf_flags(0);
            check!(!none.exec && !none.write && !none.read);
            let all = UserMemoryFlags::from_elf_flags(0b111);
            check!(all.exec && all.write && all.read);
            let high = UserMemoryFlags::from_elf_flags(0xFFFF_FFF8);
            check!(!high.exec && !high.write && !high.read, "other bits are ignored");
        }
    }
}

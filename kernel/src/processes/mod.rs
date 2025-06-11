
use alloc::{boxed::Box, collections::btree_map::BTreeMap, slice, string::String};
use elf::{abi::{ET_EXEC, ET_REL, PT_LOAD}, endian::NativeEndian, ElfBytes};
use lazy_static::lazy_static;
use x86_64::{registers::{rflags::RFlags, segmentation::GS}, structures::{idt::InterruptStackFrameValue, paging::Page}, VirtAddr};
use crate::{interrupts::GDT, memory::{user::{alloc_user_pages, UserMemoryFlags, USER_END_ADDR}, SizedPage, PAGE_SIZE}, scheduling::threads::{sync::KIntMutex, Thread}, utils::id_table::IDTable, SCHEDULER};

mod process;
pub use process::{ProcessID, Process};
/// ELF helper definitions
pub mod elfdefs;
/// Syscalls!
pub mod syscalls;

lazy_static! {
    pub static ref PROCESSES: KIntMutex<IDTable<ProcessID, Process>> =
        KIntMutex::new(IDTable::new());
}

/// Spawns a process from the given ELF data, with the given name, creating a 
/// thread that loads it into memory and jumps to the entrypoint.
///
/// Returns the PID if successful and `None` if the ELF data is invalid.
pub fn spawn_proc(name: String, elf_data: Box<[u8]>) -> Option<ProcessID> {
    // Do some quick parses to make sure the ELF is kosher and return if not
    // (actual parsing happens in the thread below)
    let elf = ElfBytes::<NativeEndian>::minimal_parse(&elf_data).ok()?;
    elf.segments()?;
    if elf.ehdr.e_type & ET_EXEC == 0 { return None; }
    if elf.ehdr.e_entry == 0 { return None; }

    let proc = Process::new(name);
    let pid = PROCESSES.lock().add_entry(proc);

    let t = Thread::new_thread(move || {
        let int_stack = {
            let elf = ElfBytes::<NativeEndian>::minimal_parse(&elf_data)
                .unwrap();

            let va_offset = if elf.ehdr.e_type & ET_REL != 0 {
                PAGE_SIZE
            } else {
                0
            };

            for segment in elf.segments().unwrap() {
                if segment.p_type == PT_LOAD {
                    let va_start = VirtAddr::new(segment.p_vaddr + va_offset);
                    let va_end = va_start + segment.p_memsz;
                    let first_page = SizedPage::containing_address(va_start);
                    let last_page = SizedPage::containing_address(va_end - 1);
                    let pages = Page::range_inclusive(first_page, last_page);
                    let flags = UserMemoryFlags::from_elf_flags(segment.p_flags);
                    alloc_user_pages(pages, flags);

                    let seg_slice = unsafe {
                        let ptr = va_start.as_mut_ptr() as *mut u8;
                        slice::from_raw_parts_mut(ptr, segment.p_memsz as usize)
                    };

                    let seg_data = elf.segment_data(&segment).unwrap();
                    seg_slice[0..seg_data.len()].copy_from_slice(seg_data);

                    // zero the rest of the segment (from the end of file data
                    // to the end of the segment)
                    seg_slice[seg_data.len()..].fill(0);
                }
            }

            // Allocate the stack
            let stack_top = VirtAddr::new(USER_END_ADDR - PAGE_SIZE);
            let stack_bottom = stack_top - (PAGE_SIZE * 4);
            let first_page = SizedPage::containing_address(stack_bottom);
            let last_page = SizedPage::containing_address(stack_top - 1);
            let pages = Page::range_inclusive(first_page, last_page);
            let flags = UserMemoryFlags {
                read: true, write: true, exec: false,
            };
            alloc_user_pages(pages, flags);

            InterruptStackFrameValue::new(
                VirtAddr::new(elf.ehdr.e_entry + va_offset),
                GDT.user_code,
                RFlags::INTERRUPT_FLAG,
                stack_top,
                GDT.user_data,
            )
        };

        // Jump to userland!
        unsafe {
            // Swap GS every time we transition from kernelland to userland
            GS::swap();
            int_stack.iretq();
        }

    }, Some(pid));
    SCHEDULER.lock().add_thread(t);

    Some(pid)
}

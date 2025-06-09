
use alloc::{boxed::Box, collections::btree_map::BTreeMap, slice, string::String};
use elf::{abi::{ET_EXEC, ET_REL, PT_LOAD}, endian::NativeEndian, ElfBytes};
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::{registers::rflags::RFlags, structures::{idt::InterruptStackFrameValue, paging::Page}, VirtAddr};
use crate::{interrupts::GDT, memory::{user::{alloc_user_pages, UserMemoryFlags, USER_END_ADDR}, SizedPage, PAGE_SIZE}, scheduling::{threads::Thread, SCHEDULER}};

mod process;
pub use process::{ProcessID, Process};
/// ELF helper definitions
pub mod elfdefs;

lazy_static! {
    pub static ref PROCESSES: Mutex<ProcessTable> =
        Mutex::new(ProcessTable::new());
}

/// Contains all processes on the system, and some metadata
pub struct ProcessTable {
    processes: BTreeMap<ProcessID, Process>,
    next_id: ProcessID,
}

impl ProcessTable {
    pub fn new() -> ProcessTable {
        ProcessTable {
            processes: BTreeMap::new(),
            next_id: 0,
        }
    }

    /// Adds a new proc to the table and returns its given PID
    pub fn add_proc(&mut self, thread: Process) -> ProcessID {
        let first_id = self.next_id;
        loop { // TODO: Optimize to not be O(n)
            let id = self.next_id;
            self.next_id += 1;
            if !self.processes.contains_key(&id) {
                self.processes.insert(id, thread);
                return id;
            }

            // We've gone through every PID and gotten back to the start
            if self.next_id == first_id {
                panic!("Out of Process IDs!");
            }
        }
    }

    /// Returns a ref to a proc by PID, if it exists
    pub fn get_proc(&self, id: ProcessID) -> Option<&Process> {
        self.processes.get(&id)
    }
    ///
    /// Returns a mutable refernce to a proc by PID, if it exists
    pub fn get_proc_mut(&mut self, id: ProcessID) -> Option<&mut Process> {
        self.processes.get_mut(&id)
    }
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
    let pid = PROCESSES.lock().add_proc(proc);

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

                    // zero the rest
                    for i in seg_data.len()..seg_slice.len() {
                        seg_slice[i] = 0;
                    }
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
            int_stack.iretq();
        }

    }, Some(pid));
    SCHEDULER.lock().add_thread(t);

    Some(pid)
}

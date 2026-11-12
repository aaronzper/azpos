
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ktest::*;
    use crate::filesystem::{FilePath, FileSystem};
    use crate::scheduling::threads::ThreadID;
    use alloc::{format, string::ToString, sync::Arc, vec, vec::Vec};
    use elf::abi::{ET_CORE, ET_DYN, PT_LOAD};
    use libtestkit::TestResult;

    /// The highest process ID and thread ID that the scans check
    const MAX_SCAN_ID: u32 = 512;

    /// The address of the code segment in the ELF files of these cases
    const CODE_ADDR: u64 = 0x40_0000;

    /// Reads the fixture program `name` from `/tests/progs` on the test disk
    pub(crate) fn test_program(name: &str) -> Option<Box<[u8]>> {
        let path = FilePath::new(format!("/tests/progs/{name}"))?;
        root_fs().read_all(&path)
    }

    /// Calls `spawn_proc` on a big stack and returns its result.
    ///
    /// In a debug build, `spawn_proc` overflows a kernel stack when `PROCESSES` is not empty.
    /// `spawn_proc_of_a_second_process_fits_on_a_kernel_stack` shows the bug.
    pub(crate) fn spawn(name: &str, data: Box<[u8]>) -> Option<ProcessID> {
        on_big_stack(|| spawn_proc(name.to_string(), data))
    }

    /// Reads the fixture program `name` and starts it as a process
    ///
    /// The read of a big file also overflows the stack of a case, so it runs on a big stack too.
    pub(crate) fn spawn_test_program(name: &str) -> Option<ProcessID> {
        let data = on_big_stack(|| test_program(name))?;
        spawn(name, data)
    }

    /// Returns the number of entries in `PROCESSES`
    fn process_count() -> usize {
        let procs = PROCESSES.lock();
        (0..MAX_SCAN_ID).filter(|id| procs.get_entry(*id).is_some()).count()
    }

    /// Returns the number of threads in the scheduler
    fn thread_count() -> usize {
        let sched = SCHEDULER.lock();
        (0..MAX_SCAN_ID).filter(|id| sched.get_thread(*id).is_some()).count()
    }

    /// Returns the ID of the first thread of process `pid`
    fn thread_of(pid: ProcessID) -> Option<ThreadID> {
        let sched = SCHEDULER.lock();
        (0..MAX_SCAN_ID).find(|id| sched.get_thread(*id).is_some_and(|t| t.proccess() == Some(pid)))
    }

    /// Returns true if `PROCESSES` has an entry for `pid`
    pub(crate) fn process_exists(pid: ProcessID) -> bool {
        PROCESSES.lock().get_entry(pid).is_some()
    }

    /// Waits until the thread of process `pid` has run 2 times
    pub(crate) fn wait_for_process_to_run(pid: ProcessID) -> TestResult {
        let tid = thread_of(pid).or_fail("thread of the process")?;
        let ran = wait_until(|| SCHEDULER.lock().get_thread(tid).is_some_and(|t| t.runs() > 1));
        check!(ran, "the thread of the process did not run");
        Ok(())
    }

    /// Machine code that calls the `Yield` syscall in a loop:
    /// `mov edi, Yield; syscall; jmp <start>`
    pub(crate) fn yield_loop_code() -> Vec<u8> {
        let mut code = vec![0xBF];
        code.extend_from_slice(&(libsci::Syscall::Yield as u32).to_le_bytes());
        code.extend_from_slice(&[0x0F, 0x05, 0xEB, 0xF7]);
        code
    }

    /// Returns a loadable segment of `code` at `CODE_ADDR` with all permissions
    fn code_segment(code: &[u8]) -> Segment<'_> {
        Segment { p_type: PT_LOAD, flags: 0b111, vaddr: CODE_ADDR, memsz: code.len() as u64, data: code }
    }

    /// Returns an ELF file of type `e_type` with one loadable segment of `code`
    pub(crate) fn elf_with_code(e_type: u16, code: &[u8]) -> Box<[u8]> {
        minimal_elf(e_type, CODE_ADDR, &[code_segment(code)])
    }

    /// Calls `spawn_proc` with `data`.
    /// Checks that it gives `None` and adds no process and no thread.
    fn check_rejected(what: &str, data: Box<[u8]>) -> TestResult {
        let procs = process_count();
        let threads = thread_count();
        let pid = spawn("bad", data);
        check!(pid.is_none(), "{what}: spawn_proc gave {pid:?}");
        let procs_after = process_count();
        let threads_after = thread_count();
        check!(procs_after == procs, "{what}: {procs} processes before, {procs_after} after");
        check!(threads_after == threads, "{what}: {threads} threads before, {threads_after} after");
        Ok(())
    }

    /// Returns a valid header of a 32-bit ELF executable with no segments
    fn elf32_header() -> Box<[u8]> {
        let mut h = Vec::new();
        h.extend_from_slice(&[0x7F, b'E', b'L', b'F', 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        h.extend_from_slice(&2u16.to_le_bytes()); // e_type: ET_EXEC
        h.extend_from_slice(&3u16.to_le_bytes()); // e_machine: x86
        h.extend_from_slice(&1u32.to_le_bytes()); // e_version
        h.extend_from_slice(&(CODE_ADDR as u32).to_le_bytes()); // e_entry
        h.extend_from_slice(&52u32.to_le_bytes()); // e_phoff
        h.extend_from_slice(&0u32.to_le_bytes()); // e_shoff
        h.extend_from_slice(&0u32.to_le_bytes()); // e_flags
        h.extend_from_slice(&52u16.to_le_bytes()); // e_ehsize
        h.extend_from_slice(&32u16.to_le_bytes()); // e_phentsize
        h.extend_from_slice(&[0; 8]); // e_phnum, e_shentsize, e_shnum, e_shstrndx
        h.into_boxed_slice()
    }

    ktest! {
        /// `minimal_elf` makes a file that `spawn_proc` accepts.
        /// The process calls the `Yield` syscall in a loop, so it is harmless.
        fn spawn_proc_accepts_a_minimal_executable() {
            let pid = spawn("minimal", elf_with_code(ET_EXEC, &yield_loop_code()));
            let pid = pid.or_fail("spawn_proc")?;
            check!(process_exists(pid));
            wait_for_process_to_run(pid)?;
        }

        fn spawn_proc_of_empty_data_gives_none() {
            check_rejected("empty", Box::new([]))?;
        }

        fn spawn_proc_of_random_bytes_gives_none() {
            let bytes: Vec<u8> = (0..200u32).map(|i| (i * 37 + 11) as u8).collect();
            check_rejected("random", bytes.into_boxed_slice())?;
        }

        fn spawn_proc_of_a_short_header_gives_none() {
            let full = elf_with_code(ET_EXEC, &yield_loop_code());
            check_rejected("20 bytes", full[..20].into())?;
            check_rejected("63 bytes", full[..63].into())?;
        }

        /// The program header table must be in the file.
        fn spawn_proc_of_a_cut_program_header_gives_none() {
            let full = elf_with_code(ET_EXEC, &yield_loop_code());
            check_rejected("header only", full[..64].into())?;
            check_rejected("half a program header", full[..64 + 28].into())?;
        }

        /// The kernel runs only x86-64 code, so a 32-bit file is not valid.
        /// Today `spawn_proc` accepts the file. The process then faults, and the kernel panics.
        #[known_bug(unfiled, crash)]
        fn spawn_proc_of_a_32_bit_elf_gives_none() {
            check_rejected("32-bit", elf32_header())?;
        }

        fn spawn_proc_of_a_big_endian_elf_gives_none() {
            let mut data = elf_with_code(ET_EXEC, &yield_loop_code());
            data[5] = 2; // EI_DATA: big-endian
            check_rejected("big-endian", data)?;
        }

        fn spawn_proc_of_a_bad_magic_number_gives_none() {
            let mut data = elf_with_code(ET_EXEC, &yield_loop_code());
            data[1] = b'X';
            check_rejected("bad magic number", data)?;
        }

        fn spawn_proc_with_entry_zero_gives_none() {
            let code = yield_loop_code();
            check_rejected("entry 0", minimal_elf(ET_EXEC, 0, &[code_segment(&code)]))?;
        }

        fn spawn_proc_of_a_relocatable_core_or_none_file_gives_none() {
            check_rejected("ET_REL", elf_with_code(ET_REL, &yield_loop_code()))?;
            check_rejected("ET_CORE", elf_with_code(ET_CORE, &yield_loop_code()))?;
            check_rejected("ET_NONE", elf_with_code(0, &yield_loop_code()))?;
        }

        /// The loader does not relocate code, so it must refuse a
        /// position-independent file (GitHub #41).
        /// Today it accepts the file, and the process spins in `jmp .`.
        #[known_bug(41, crash)]
        fn spawn_proc_of_a_position_independent_file_gives_none() {
            check_rejected("ET_DYN", elf_with_code(ET_DYN, &[0xEB, 0xFE]))?;
        }

        /// Only `ET_EXEC` (2) is an executable.
        /// The code of the file yields in a loop, so a process that starts is harmless.
        /// Today `spawn_proc` tests one bit of `e_type`, so it accepts each type with bit 1 set.
        #[known_bug(unfiled)]
        fn spawn_proc_of_an_os_specific_type_gives_none() {
            check_rejected("e_type 6", elf_with_code(6, &yield_loop_code()))?;
            check_rejected("e_type 0xFE02", elf_with_code(0xFE02, &yield_loop_code()))?;
        }

        /// A user program must not map kernel memory.
        /// Today `spawn_proc` accepts the file. Its thread then panics in `alloc_user_pages`.
        #[known_bug(unfiled, crash)]
        fn spawn_proc_of_a_segment_in_kernel_memory_gives_none() {
            let code = yield_loop_code();
            let mut segment = code_segment(&code);
            segment.vaddr = crate::memory::KERNEL_START_ADDR;
            check_rejected("kernel segment", minimal_elf(ET_EXEC, CODE_ADDR, &[segment]))?;
        }

        /// A segment with a file size above its memory size is not valid.
        /// Today `spawn_proc` accepts the file. Its thread then panics in the copy of the segment.
        #[known_bug(unfiled, crash)]
        fn spawn_proc_of_a_segment_with_filesz_above_memsz_gives_none() {
            let code = yield_loop_code();
            let mut segment = code_segment(&code);
            segment.memsz = 1;
            check_rejected("filesz > memsz", minimal_elf(ET_EXEC, CODE_ADDR, &[segment]))?;
        }

        /// The fixture program `yield` starts as a process, and its thread runs.
        fn spawn_proc_of_the_yield_program_runs_it() {
            let pid = spawn_test_program("yield").or_fail("spawn yield")?;
            check!(process_exists(pid));
            wait_for_process_to_run(pid)?;
        }

        /// Two processes get different IDs.
        fn spawn_proc_gives_a_new_id_to_each_process() {
            let code = yield_loop_code();
            let a = spawn("a", elf_with_code(ET_EXEC, &code)).or_fail("spawn a")?;
            let b = spawn("b", elf_with_code(ET_EXEC, &code)).or_fail("spawn b")?;
            check_ne!(a, b);
            check!(process_exists(a) && process_exists(b));
        }

        /// A new kernel thread can start 2 processes on its own stack of 16 KiB.
        /// Today the insert into `PROCESSES` overflows the stack, and the CPU gives a double fault.
        #[known_bug(unfiled, crash)]
        fn spawn_proc_of_a_second_process_fits_on_a_kernel_stack() {
            let count = Arc::new(spin::Mutex::new(0));
            let count_in_thread = Arc::clone(&count);
            run_in_thread(move || {
                for name in ["first", "second"] {
                    if spawn_proc(name.to_string(), elf_with_code(ET_EXEC, &yield_loop_code())).is_some() {
                        *count_in_thread.lock() += 1;
                    }
                }
            });
            check_eq!(*count.lock(), 2);
        }
    }
}

use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String};
use elf::{endian::NativeEndian, ElfBytes};
use lazy_static::lazy_static;
use spin::Mutex;
use crate::scheduling::{thread_yield, threads::Thread, SCHEDULER};

mod process;
pub use process::{ProcessID, Process};

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
    // Do a quick parse to make sure the ELF is kosher and return if not (actual
    // parsing happens in the thread below)
    ElfBytes::<NativeEndian>::minimal_parse(&elf_data).ok()?;

    let proc = Process::new(name);
    let pid = PROCESSES.lock().add_proc(proc);

    let t = Thread::new_thread(move || {
        let elf = ElfBytes::<NativeEndian>::minimal_parse(&elf_data).unwrap();

        println!("Hi from proc {pid}");
    }, Some(pid));
    SCHEDULER.lock().add_thread(t);

    Some(pid)
}

use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::{Mutex, MutexGuard};
use threads::{Thread, ThreadID, ThreadTable};

/// Threads
pub mod threads;

lazy_static! {
    pub static ref SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());
}

/// The main scheduler, in charge of scheduling kernel and user threads
pub struct Scheduler {
    /// A map owning all threads and allocating their IDs
    threads: ThreadTable,
    /// A list of all runable threads
    runnable: Vec<ThreadID>,
    /// The current thread that is running. If this is `None`, the scheduler
    /// hasnt been started yet
    currently_running: Option<ThreadID>,
}

impl Scheduler {
    /// Creates the `Scheduler` with no threads to run
    fn new() -> Scheduler {
        Scheduler {
            threads: ThreadTable::new(),
            runnable: Vec::new(),
            currently_running: None,
        }
    }
    
    /// Add a thread to the runnable queue and returns its new ID
    pub fn add_thread(&mut self, thread: Thread) -> ThreadID {
        let id = self.threads.add_thread(thread);
        self.runnable.push(id);
        id
    }

    /// Get a thread by ID if it exists
    pub fn get_thread(&self, id: ThreadID) -> Option<&Thread> {
        self.threads.get_thread(id)
    }

    /// Get a mutable thread by ID if it exists
    pub fn get_thread_mut(&mut self, id: ThreadID) -> Option<&mut Thread> {
        self.threads.get_thread_mut(id)
    }

    /// Returns the ID of the currently running thread.
    ///
    /// Panics if the scheduler hasnt been started
    pub fn currently_running(&self) -> ThreadID {
        self.currently_running.expect("Scheduler not started!")
    }

    /// Starts the scheduler by running a thread! This should only be run at the
    /// end of `kmain()`.
    ///
    /// Panics if there are no threads to run or they've all been started
    ///
    /// Unsafe cause it changes control to any thread and never returns (spooky)
    pub unsafe fn start(&mut self) -> ! {
        let id_to_start = self.runnable.iter()
            .find(|id| {
                let t = self.get_thread(**id).unwrap();
                !t.started()
            });

        match id_to_start {
            Some(id) => {
                self.currently_running = Some(*id);
                let t = self.get_thread_mut(*id).unwrap();
                unsafe {
                    t.start()
                }
            }

            None => panic!("No threads to start")
        }
    }
}

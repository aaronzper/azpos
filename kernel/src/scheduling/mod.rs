use core::cmp::Ordering;

use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;
use threads::{state::CpuState, Thread, ThreadID, ThreadTable};

use crate::devices::pic::PICInterrupt;

/// Threads
pub mod threads;

lazy_static! {
    pub static ref SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());
}

fn wait_loop() {
    loop {
        crate::interrupts::wait();
    }
}

/// The main scheduler, in charge of scheduling kernel and user threads
pub struct Scheduler {
    /// A map owning all threads and allocating their IDs
    threads: ThreadTable,
    /// A list of all runable threads
    runnable: Vec<ThreadID>,
    /// The current thread that is running. If this is `None`, the scheduler
    /// hasnt been started yet or is idling
    currently_running: Option<ThreadID>,
    /// A dummy wait thread that just idles. Used if there are no other threads
    /// to run
    idle_thread: Thread
}

impl Scheduler {
    /// Creates the `Scheduler` with no threads to run
    fn new() -> Scheduler {
        Scheduler {
            threads: ThreadTable::new(),
            runnable: Vec::new(),
            currently_running: None,
            idle_thread: Thread::new_thread(wait_loop, None),
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
    pub fn currently_running(&self) -> Option<ThreadID> {
        self.currently_running
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

    /// Runs a round of the scheduler, picking a new thread to run
    ///
    /// `state` needs to be a ref to the `CpuState` that spawned this interrupt,
    /// allowing the scheduler to save it to the old thread and update it
    /// with that of the new one.
    pub unsafe fn schedule(&mut self, state: &mut CpuState) {
        let new_id = self.runnable.iter()
            .min_by(|id_a, id_b| {
                let t_a = self.get_thread(**id_a).unwrap();
                let t_b = self.get_thread(**id_b).unwrap();
                let runs_a = t_a.runs();
                let runs_b = t_b.runs();

                if runs_a < runs_b {
                    Ordering::Less
                } else if runs_a == runs_b {
                    Ordering::Equal
                } else {
                    Ordering::Greater
                }
            })
            .cloned();

        let old_t = match self.currently_running() {
            Some(id) => {
                self.get_thread_mut(id).unwrap()
            },
            None => {
                &mut self.idle_thread
            }
        };
        old_t.state = state.clone();

        let new_t = match new_id {
            Some(id) => {
                self.currently_running = Some(id);
                self.get_thread_mut(id).unwrap()
            },
            None => {
                self.currently_running = None;
                &mut self.idle_thread
            },
        };
        new_t.add_run();
        *state = new_t.state.clone();
    }

    /// Blocks the given thread and returns a `BlockedThread`.
    ///
    /// Panics if the thread isnt currently runnable
    pub fn block_thread(&mut self, thread: ThreadID) -> BlockedThread {
        match self.runnable.iter().position(|tid| *tid == thread) {
            Some(i) => self.runnable.remove(i),
            None => panic!("Thread {} isn't currently runnable!", thread),
        };

        BlockedThread { thread }
    }

    fn unblock_thread(&mut self, thread: ThreadID) {
        self.runnable.push(thread);
    }
}

/// An object representing ownership over a blocked thread. Unblocks the thread
/// when it goes out of scope.
pub struct BlockedThread {
    thread: ThreadID,
}

impl Drop for BlockedThread {
    fn drop(&mut self) {
        SCHEDULER.lock().unblock_thread(self.thread);
    }
}

/// Yields control back to the scheduler
///
/// (Right now this just raises a timer interrupt to run the scheduler but once
/// i have a "yield" syscall I'll use that)
///
/// TODO: Panic if we're not running in thread (e.g. an ISR)
pub fn kthread_yield() {
    unsafe {
        // Kinda jank but raise a timer interrupt to "yield"
        x86_64::instructions::interrupts::
            software_interrupt::<{PICInterrupt::Timer as u8}>();
    }
}

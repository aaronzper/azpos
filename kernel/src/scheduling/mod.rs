use core::cmp::Ordering;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use threads::{state::CpuState, sync::KIntMutex, Thread, ThreadID, ThreadTable};
use x86_64::registers::segmentation::GS;
use crate::{devices::pic::PICInterrupt, processes::{syscalls::set_syscall_stack, ProcessID, PROCESSES}};

/// Threads
pub mod threads;

lazy_static! {
    pub static ref SCHEDULER: KIntMutex<Scheduler> = KIntMutex::new(Scheduler::new());
}

fn wait_loop() {
    loop {
        crate::interrupts::wait();
    }
}

enum SchedulerState {
    /// The scheduler has not been started
    NotStarted,
    /// The scheduler is running the idle thread
    Idle,
    /// The scheduler is running this thread
    Running(ThreadID),
}

/// The main scheduler, in charge of scheduling kernel and user threads
pub struct Scheduler {
    /// A map owning all threads and allocating their IDs
    threads: ThreadTable,
    /// A list of all runable threads
    runnable: Vec<ThreadID>,
    /// What the scheduler is currently up to. See rustdoc on `SchedulerState`
    /// for more info.
    status: SchedulerState,
    /// A dummy wait thread that just idles. Used if there are no other threads
    /// to run
    idle_thread: Thread,
    /// The ~thread graveyard~ (spooky)
    ///
    /// Threads that are due to be killed (pushed to be `kill_thread`). Processed
    /// on each schedule call.
    graveyard: Vec<ThreadID>,
}

impl Scheduler {
    /// Creates the `Scheduler` with no threads to run
    fn new() -> Scheduler {
        Scheduler {
            threads: ThreadTable::new(),
            runnable: Vec::new(),
            status: SchedulerState::NotStarted,
            idle_thread: Thread::new_thread(wait_loop, None),
            graveyard: Vec::new(),
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

    /// Returns the ID of the currently running thread, if the scheduler is
    /// running one
    pub fn currently_running(&self) -> Option<ThreadID> {
        match self.status {
            SchedulerState::Running(tid) => Some(tid),
            _ => None,
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

        // Save state of the old thread, and track if we're coming out of
        // userland (so we know whether to swap GS)
        let was_user = match self.status {
            SchedulerState::Running(tid) => {
                let old_t = self.get_thread_mut(tid).unwrap();
                old_t.state = state.clone();

                match old_t.proccess() {
                    Some(old_pid) => {
                        let mut procs_lock = PROCESSES.lock();
                        let old_p = procs_lock.get_proc_mut(old_pid).unwrap();
                        old_p.save_page_tables();

                        // User process threads are usually in userland, but
                        // if they get preempted (or yield) in a syscall,
                        // they're coming from kernelland, so check RIP to be
                        // sure
                        old_t.state.is_user()
                    },

                    // Kernel threads are (obviously) always in kernelland
                    None => false,
                }
            },

            SchedulerState::Idle => {
                self.idle_thread.state = state.clone();
                // Idle thread is always in kernalland
                false
            },

            // If we're not started then obviously not coming form userlland
            SchedulerState::NotStarted => false,
        };

        // Load in state of the new thread
        let new_t = match new_id {
            Some(id) => {
                self.status = SchedulerState::Running(id);
                let new_t = self.get_thread_mut(id).unwrap();

                match new_t.proccess() {
                    Some(new_pid) => {
                        let mut procs_lock = PROCESSES.lock();
                        let new_p = procs_lock.get_proc_mut(new_pid).unwrap();
                        new_p.load_page_tables();

                        // Save to GS for syscalls
                        set_syscall_stack(new_t.stack_top());
                    },

                    None => (),
                };

                new_t
            },
            None => {
                self.status = SchedulerState::Idle;
                &mut self.idle_thread
            },
        };
        new_t.add_run();
        *state = new_t.state.clone();

        // If we're leaving user and entering kernel, or vice versa,
        // swap GS
        if new_t.state.is_user() != was_user {
            unsafe { GS::swap() };
        }

        // Kill threads in the ~graveyard~
        for dead_tid in self.graveyard.drain(..) {
            self.threads.remove_thread(dead_tid).unwrap();
        }
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
        // Make sure the thread still exists and isnt queued to die
        // (a thread coulda been killed while also blocked)
        if self.threads.get_thread(thread).is_some() && !self.graveyard.contains(&thread) {
            self.runnable.push(thread);
        }
    }

    /// Terminates the given thread, removing it from the run queue and queuing
    /// it to ~die~ on the next schedule run (not doing it immediately prevents
    /// a currently-running thread to find itself on a freed stack, or other
    /// funky stuff).
    ///
    /// Unsafe cause if you call this on a thread in the middle of running, its
    /// stack variables never get cleaned up.
    ///
    /// Returns `None` if the given TID is invalid.
    unsafe fn kill_thread(&mut self, thread: ThreadID) -> Option<()> {
        // Check if the thread exists
        self.threads.get_thread(thread)?;

        // Queue it to ~die~
        self.graveyard.push(thread);

        // Remove it from run queue, if not blocked
        match self.runnable.iter().position(|tid| *tid == thread) {
            Some(i) => { self.runnable.remove(i); },
            None => (),
        };

        Some(())
    }

    /// Returns the currently running PID, if we're in a user thread. `None` otherwise.
    pub fn current_proc(&self) -> Option<ProcessID> {
        let tid = self.currently_running()?;
        let t = self.get_thread(tid).unwrap();
        t.proccess()
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
pub fn thread_yield() {
    unsafe {
        // Kinda jank but raise a timer interrupt to "yield"
        x86_64::instructions::interrupts::
            software_interrupt::<{PICInterrupt::Timer as u8}>();
    }
}

/// Kills the current thread and yields back to the scheduler
///
/// Unsafe cause calling this without unwinding the stack can leak resources
///
/// For the time being, should only be called at the bottom of the thread stack,
/// since I haven't implemented unwinding
///
/// Panics if we're not in a thread
pub unsafe fn thread_exit() -> ! {
    let mut sched = SCHEDULER.lock();
    let tid = sched.currently_running().unwrap();
    unsafe { sched.kill_thread(tid); }
    drop(sched);

    thread_yield();
    panic!("Returned after exit");
}

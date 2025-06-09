use alloc::vec::Vec;

use crate::scheduling::{thread_yield, BlockedThread, SCHEDULER};

use super::{KMutex, KMutexGuard};

/// A kernel condition variable that can be used to block a kthread waiting for
/// some event to occur
pub struct KCondvar {
    blocked_threads: KMutex<Vec<BlockedThread>>
}

impl KCondvar {
    pub const fn new() -> Self {
        KCondvar { 
            blocked_threads: KMutex::new(Vec::new())
        }
    }

    /// Blocks the current thread until release by a call to `notify_all`.
    pub fn wait<'a, T>(&self, guard: KMutexGuard<'a, T>) -> KMutexGuard<'a, T> {
        let mut sched = SCHEDULER.lock();

        let mut blocks = self.blocked_threads.lock();
        let tid = sched.currently_running().expect("Scheduler not running");
        let block = sched.block_thread(tid);
        blocks.push(block);
        drop(blocks);

        let mutex = guard.into_inner_mutex();
        drop(sched);

        thread_yield();
        mutex.lock()
    }

    /// Unblocks all threads that have been blocked by a `wait()` call
    pub fn notify_all(&self) {
        self.blocked_threads.lock().clear();
    }
}

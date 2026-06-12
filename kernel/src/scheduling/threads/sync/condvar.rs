use alloc::vec::Vec;

use crate::scheduling::{thread_yield, BlockedThread, SCHEDULER};

use super::{KMutex, KMutexGuard};

/// A kernel condition variable that blocks threads until an event occurs.
///
/// # Locking convention
/// The caller of [`notify_all`](KCondvar::notify_all) **must** hold the
/// `KMutex` that protects the associated condition when it calls
/// `notify_all`.  Failing to do so opens a lost-wakeup window: the waiter
/// can observe the condition, release the mutex, and be scheduled out before
/// it adds itself to `blocked_threads`; a notify in that gap would clear an
/// empty list and the waiter would sleep forever.
///
/// # IRQ safety
/// `KCondvar` is **not** IRQ-safe: `notify_all` and `wait` both take a
/// `KMutex` internally, which can block.  Use [`KIntMutex`](super::KIntMutex)
/// + [`Buffer`](super::Buffer) for producer/consumer patterns that involve
/// interrupt handlers.
pub struct KCondvar {
    blocked_threads: KMutex<Vec<BlockedThread>>,
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

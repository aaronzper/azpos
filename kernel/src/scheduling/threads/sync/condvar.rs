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

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use x86_64::instructions::interrupts::without_interrupts;
    use crate::ktest::*;

    /// Returns the number of threads that wait on `condvar`.
    ///
    /// The read runs with interrupts off. A waiter takes the same lock in
    /// `wait` with interrupts off, so the two cannot overlap.
    pub fn waiters(condvar: &KCondvar) -> usize {
        without_interrupts(|| condvar.blocked_threads.lock().len())
    }

    ktest! {
        /// `notify_all` with no waiter does nothing.
        fn notify_all_with_no_waiter_does_nothing() {
            let condvar = KCondvar::new();
            condvar.notify_all();
            check_eq!(waiters(&condvar), 0);
        }

        /// `wait` releases the mutex. After `notify_all`, the waiter holds it again.
        fn wait_releases_the_mutex_and_notify_wakes() {
            let mutex = Arc::new(KMutex::new(0));
            let condvar = Arc::new(KCondvar::new());
            let woke = Arc::new(AtomicBool::new(false));
            let held_after_wake = Arc::new(AtomicBool::new(false));

            let (m, c, w, h) = (Arc::clone(&mutex), Arc::clone(&condvar),
                Arc::clone(&woke), Arc::clone(&held_after_wake));
            spawn_thread(move || {
                let guard = m.lock();
                let guard = c.wait(guard);
                h.store(m.try_lock().is_none(), Ordering::SeqCst);
                drop(guard);
                w.store(true, Ordering::SeqCst);
            });

            let waiting = wait_until(|| waiters(&condvar) == 1);
            let free_while_waiting = mutex.try_lock().is_some();
            let woke_early = woke.load(Ordering::SeqCst);
            condvar.notify_all();
            let woke_after = wait_until(|| woke.load(Ordering::SeqCst));

            check!(waiting, "the thread did not wait");
            check!(free_while_waiting, "wait did not release the mutex");
            check!(!woke_early);
            check!(woke_after);
            check!(held_after_wake.load(Ordering::SeqCst));
        }

        /// `notify_all` wakes each of three waiters.
        fn notify_all_wakes_three_waiters() {
            let mutex = Arc::new(KMutex::new(()));
            let condvar = Arc::new(KCondvar::new());
            let woke = Arc::new(AtomicUsize::new(0));
            let mut waiting = true;
            // Each thread starts after the one before it waits.
            // Thus no thread waits on the mutex when another thread calls `wait`.
            for n in 1..=3 {
                let (m, c, w) = (Arc::clone(&mutex), Arc::clone(&condvar), Arc::clone(&woke));
                spawn_thread(move || {
                    let guard = m.lock();
                    drop(c.wait(guard));
                    w.fetch_add(1, Ordering::SeqCst);
                });
                waiting &= wait_until(|| waiters(&condvar) == n);
            }

            condvar.notify_all();
            let all_woke = wait_until(|| woke.load(Ordering::SeqCst) == 3);

            check!(waiting, "{} of 3 threads wait", waiters(&condvar));
            check!(all_woke, "{} of 3 threads woke", woke.load(Ordering::SeqCst));
        }

        /// `wait` works when another thread waits for the mutex.
        /// Today `wait` unlocks the mutex while it holds the scheduler lock.
        /// The unlock wakes the other thread, which takes the scheduler lock
        /// again, so the kernel hangs.
        #[known_bug(18, crash)]
        #[timeout(10)]
        fn wait_with_a_thread_blocked_on_the_mutex_does_not_hang() {
            let mutex = Arc::new(KMutex::new(()));
            let condvar = Arc::new(KCondvar::new());
            let locker_done = Arc::new(AtomicBool::new(false));

            let c = Arc::clone(&condvar);
            spawn_thread(move || {
                wait_until(|| waiters(&c) == 1);
                c.notify_all();
            });

            let guard = mutex.lock();
            let (m, d) = (Arc::clone(&mutex), Arc::clone(&locker_done));
            spawn_thread(move || {
                drop(m.lock());
                d.store(true, Ordering::SeqCst);
            });
            let blocked = wait_until(|| mutex.waiters() == 1);

            drop(condvar.wait(guard));
            let done = wait_until(|| locker_done.load(Ordering::SeqCst));
            check!(blocked, "the second thread did not block");
            check!(done);
        }
    }
}

use core::{cell::UnsafeCell, ops::{Deref, DerefMut}, sync::atomic::{AtomicBool, Ordering}};use alloc::vec::Vec;
use crate::scheduling::{thread_yield, BlockedThread, SCHEDULER};
use super::KIntMutex;

/// A kernel mutex that blocks until able to acquire the lock
pub struct KMutex<T> {
    value: UnsafeCell<T>,
    locked: AtomicBool,
    blocks: KIntMutex<Vec<BlockedThread>>,
}
    
unsafe impl<T> Sync for KMutex<T> {}
unsafe impl<T> Send for KMutex<T> {}

impl<T> KMutex<T> {
    pub const fn new(value: T) -> KMutex<T> {
        KMutex {
            value: UnsafeCell::new(value),
            locked: AtomicBool::new(false),
            blocks: KIntMutex::new(Vec::new()),
        }
    }

    /// Attempts to acquire the lock
    pub fn try_lock<'a>(&'a self) -> Option<KMutexGuard<'a, T>> {
        match self.locked.compare_exchange(
            false, 
            true, 
            Ordering::SeqCst, 
            Ordering::SeqCst) {

            // Got the lock!
            Ok(previous_val) => {
                assert!(previous_val == false);
                let guard = KMutexGuard::new(self);
                Some(guard)
            }

            // Didnt get the lock :(
            Err(previous_val) => {
                assert!(previous_val == true);

                None
            }
        }
    }

    /// Aqcuires the lock, blocking and yielding to the scheduler until
    /// its available
    pub fn lock<'a>(&'a self) -> KMutexGuard<'a, T> {
        loop {
            match self.try_lock() {
                Some(g) => break g,
                None => {
                    let mut blocks = self.blocks.lock();
                    let mut sched = SCHEDULER.lock();
                    let tid = sched.currently_running().expect("No scheduler");
                    let block = sched.block_thread(tid);
                    blocks.push(block);
                    drop(sched);
                    drop(blocks);
                    thread_yield();
                },
            }
        }
    }

    fn unlock(&self) {
        self.locked.store(false, Ordering::SeqCst);
        self.blocks.lock().clear();
    }
}

pub struct KMutexGuard<'a, T> {
    mutex: &'a KMutex<T>,
}

impl<'a, T> KMutexGuard<'a, T> {
    pub fn new(mutex: &'a KMutex<T>) -> Self {
        KMutexGuard { 
            mutex,
        }
    }

    /// Consumes self and returns the inner mutex, unlocking it in the process
    pub(super) fn into_inner_mutex(self) -> &'a KMutex<T> {
        self.mutex
    }
}

impl<T> Drop for KMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}


impl<'a, T> Deref for KMutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<'a, T> DerefMut for KMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.value.get() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;
    use core::sync::atomic::AtomicUsize;
    use crate::ktest::*;

    impl<T> KMutex<T> {
        /// Returns the number of threads that wait for the lock
        pub(crate) fn waiters(&self) -> usize {
            self.blocks.lock().len()
        }
    }

    ktest! {
        /// `lock` gives access. The drop of the guard releases the lock.
        fn lock_gives_access_and_drop_releases() {
            let mutex = KMutex::new(1);
            {
                let mut guard = mutex.lock();
                *guard += 1;
            }
            check!(!mutex.locked.load(Ordering::SeqCst));
            let guard = mutex.try_lock().or_fail("try_lock after drop")?;
            check_eq!(*guard, 2);
        }

        /// `try_lock` of a held lock gives `None`.
        fn try_lock_of_a_held_lock_gives_none() {
            let mutex = KMutex::new(());
            let guard = mutex.lock();
            let second = mutex.try_lock().is_none();
            drop(guard);
            check!(second);
        }

        /// A second thread blocks in `lock` until the first thread unlocks.
        fn second_locker_blocks_until_unlock() {
            let mutex = Arc::new(KMutex::new(()));
            let got_lock = Arc::new(AtomicBool::new(false));
            let guard = mutex.lock();

            let (m, flag) = (Arc::clone(&mutex), Arc::clone(&got_lock));
            spawn_thread(move || {
                let _g = m.lock();
                flag.store(true, Ordering::SeqCst);
            });

            let blocked = wait_until(|| mutex.blocks.lock().len() == 1);
            let got_while_held = got_lock.load(Ordering::SeqCst);
            drop(guard);
            let got_after = wait_until(|| got_lock.load(Ordering::SeqCst));

            check!(blocked, "the second thread did not block");
            check!(!got_while_held);
            check!(got_after);
        }

        /// Four threads add to a counter under the lock. No addition is lost.
        /// The main thread locks and unlocks in a loop, so a lost wakeup
        /// cannot hang the case.
        #[timeout(30)]
        fn four_threads_share_a_counter() {
            let counter = Arc::new(KMutex::new(0usize));
            let done = Arc::new(AtomicUsize::new(0));
            for _ in 0..4 {
                let (c, d) = (Arc::clone(&counter), Arc::clone(&done));
                spawn_thread(move || {
                    for _ in 0..200 {
                        let mut guard = c.lock();
                        let old = *guard;
                        thread_yield();
                        *guard = old + 1;
                    }
                    d.fetch_add(1, Ordering::SeqCst);
                });
            }
            let all_done = wait_until(|| {
                drop(counter.lock());
                done.load(Ordering::SeqCst) == 4
            });
            check!(all_done, "{} of 4 threads complete", done.load(Ordering::SeqCst));
            check_eq!(*counter.lock(), 800);
        }
    }
}

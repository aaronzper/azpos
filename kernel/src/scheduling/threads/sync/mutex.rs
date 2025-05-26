use core::{cell::UnsafeCell, ops::{Deref, DerefMut}, sync::atomic::{AtomicBool, Ordering}};
use alloc::vec::Vec;
use spin::Mutex;

use crate::scheduling::{kthread_yield, BlockedThread, SCHEDULER};

/// A kernel mutex that blocks until able to acquire the lock
pub struct KMutex<T> {
    value: UnsafeCell<T>,
    locked: AtomicBool,
    blocks: Mutex<Vec<BlockedThread>>,
}
    
unsafe impl<T> Sync for KMutex<T> {}
unsafe impl<T> Send for KMutex<T> {}

impl<T> KMutex<T> {
    pub const fn new(value: T) -> KMutex<T> {
        KMutex {
            value: UnsafeCell::new(value),
            locked: AtomicBool::new(false),
            blocks: Mutex::new(Vec::new()),
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
                    kthread_yield();
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

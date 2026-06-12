use core::mem::MaybeUninit;
use crate::scheduling::{thread_yield, BlockedThread, SCHEDULER};
use super::KIntMutex;

struct Inner<T, const S: usize> {
    ring: [MaybeUninit<T>; S],
    head: usize,
    len: usize,
    /// Blocked thread waiting for data.  At most one at a time — SPSC contract.
    waiting_reader: Option<BlockedThread>,
}

/// A bounded FIFO buffer safe for use between a single IRQ producer and a
/// single blocking consumer.
///
/// # SPSC contract
/// At most one thread may call [`pop`](Buffer::pop) at a time.
///
/// # IRQ safety
/// [`push`](Buffer::push) holds only a [`KIntMutex`] (no blocking, no
/// [`KMutex`](super::KMutex), no yield) and drops the unblocked
/// [`BlockedThread`] *after* releasing the lock, so `push` is safe from
/// interrupt handlers.  The lock order for both paths is:
/// `inner` (KIntMutex) → `SCHEDULER` (KIntMutex via BlockedThread::drop).
pub struct Buffer<T, const S: usize> {
    inner: KIntMutex<Inner<T, S>>,
}

unsafe impl<T: Send, const S: usize> Sync for Buffer<T, S> {}
unsafe impl<T: Send, const S: usize> Send for Buffer<T, S> {}

impl<T, const S: usize> Buffer<T, S> {
    pub const fn new() -> Self {
        Buffer {
            inner: KIntMutex::new(Inner {
                ring: [const { MaybeUninit::uninit() }; S],
                head: 0,
                len: 0,
                waiting_reader: None,
            }),
        }
    }

    /// Writes a value into the buffer, overwriting the oldest item when full.
    ///
    /// IRQ-safe: the unblocked waiter is dropped *after* the inner lock is
    /// released so `BlockedThread::drop` never nests inside `inner`.
    pub fn push(&self, value: T) {
        let waiter = {
            let mut inner = self.inner.lock();
            let tail = (inner.head + inner.len) % S;
            inner.ring[tail].write(value);
            if inner.len == S {
                // Full — overwrite oldest, advance head
                inner.head = (inner.head + 1) % S;
            } else {
                inner.len += 1;
            }
            inner.waiting_reader.take()
            // inner guard drops here, re-enabling interrupts
        };
        // BlockedThread::drop → SCHEDULER.lock(); safe because inner is released above
        drop(waiter);
    }

    /// Pops the oldest value without blocking. Returns `None` if empty.
    pub fn try_pop(&self) -> Option<T> {
        let mut inner = self.inner.lock();
        if inner.len == 0 {
            return None;
        }
        let val = unsafe { inner.ring[inner.head].assume_init_read() };
        inner.head = (inner.head + 1) % S;
        inner.len -= 1;
        Some(val)
    }

    /// Pops the oldest value, blocking until one is available.
    pub fn pop(&self) -> T {
        loop {
            {
                let mut inner = self.inner.lock();
                if inner.len > 0 {
                    let val = unsafe { inner.ring[inner.head].assume_init_read() };
                    inner.head = (inner.head + 1) % S;
                    inner.len -= 1;
                    return val;
                }
                // Queue is empty.  Register as waiting reader before releasing
                // inner so push() cannot miss the wakeup.
                // Lock order: inner (KIntMutex) → SCHEDULER (KIntMutex).
                let mut sched = SCHEDULER.lock();
                let tid = sched.currently_running().expect("No scheduler");
                let block = sched.block_thread(tid);
                inner.waiting_reader = Some(block);
                drop(sched);
                // inner guard drops here, re-enabling interrupts
            }
            thread_yield();
        }
    }
}

impl<T, const S: usize> Drop for Buffer<T, S> {
    fn drop(&mut self) {
        while self.try_pop().is_some() {}
    }
}

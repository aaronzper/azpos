use core::{cell::UnsafeCell, mem::MaybeUninit, sync::atomic::{AtomicBool, AtomicUsize, Ordering}};
use super::{KCondvar, KMutex};

/// A lockless single-consumer/single-producer FIFO ring buffer.
pub struct Buffer<T, const S: usize> {
    data: UnsafeCell<[MaybeUninit<T>; S]>,
    head: AtomicUsize,
    tail: AtomicUsize,
    len: AtomicUsize,

    reader_waiting: AtomicBool,
    readable_mtx: KMutex<()>,
    readable: KCondvar,
}

unsafe impl<T: Send, const S: usize> Sync for Buffer<T, S> {}
unsafe impl<T: Send, const S: usize> Send for Buffer<T, S> {}


impl<T, const S: usize> Buffer<T, S> {
    pub const fn new() -> Self {
        let buffer = [ const { MaybeUninit::<T>::uninit() }; S];
        
        Self {
            data: UnsafeCell::new(buffer),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            len: AtomicUsize::new(0),

            reader_waiting: AtomicBool::new(false),
            readable_mtx: KMutex::new(()),
            readable: KCondvar::new(),
        }
    }

    unsafe fn get_index(&self, idx: usize) -> T {
        assert!(idx < S);

        let ptr = self.data.get() as *mut MaybeUninit<T>;

        unsafe { 
            ptr.add(idx).as_ref().unwrap().assume_init_read()
        }
    }

    fn set_index(&self, idx: usize, val: T) {
        assert!(idx < S);

        let ptr = self.data.get() as *mut MaybeUninit<T>;

        let val_mut = unsafe { 
            ptr.add(idx).as_mut().unwrap()
        };

        *val_mut = MaybeUninit::new(val);
    }

    /// Writes a value to the buffer, overwriting the oldest one if full
    pub fn push(&self, value: T) {
        let len = self.len.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Relaxed);
        let next = (tail + 1) % S;

        let head = self.head.load(Ordering::Acquire);
        if tail == head && len != 0 { // Push the head if overflow
            self.head.store((head + 1) % S, Ordering::Release);
        }

        self.set_index(tail, value);
        self.tail.store(next, Ordering::Release);

        // Once len hits the size of the buffer, dont increase it further cause
        // at this poitn we're overwriting, not adding
        if len < S {
            self.len.store(len + 1, Ordering::Release);
        }

        if self.reader_waiting.swap(false, Ordering::Acquire) {
            self.readable.notify_all();
        }
    }

    /// Pops off the value from the start of the buffer. Returns `None` if
    /// there's nothing to be read
    pub fn try_pop(&self) -> Option<T> {
        let len = self.len.load(Ordering::Acquire);
        if len == 0 {
            return None;
        }

        let head = self.head.load(Ordering::Relaxed);

        let val = unsafe { self.get_index(head) };
        let next = (head + 1) % S;
        self.head.store(next, Ordering::Release);
        self.len.store(len - 1, Ordering::Release);

        Some(val)
    }

    /// Pops off the value from the start of the buffer. Blocks until there's
    /// something to be read
    pub fn pop(&self) -> T {
        loop {
            match self.try_pop() {
                Some(v) => break v,
                None => {
                    self.reader_waiting.store(true, Ordering::Release);
                    let lock = self.readable_mtx.lock();
                    match self.try_pop() {
                        Some(v) => break v,
                        None => self.readable.wait(lock),
                    };
                },
            }
        }
    }
}

impl<T, const S: usize> Drop for Buffer<T, S> {
    fn drop(&mut self) {
        while self.try_pop().is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{sync::Arc, vec::Vec};
    use crate::ktest::*;
    use super::super::condvar::tests::waiters;

    fn drain<T, const S: usize>(buffer: &Buffer<T, S>) -> Vec<T> {
        let mut items = Vec::new();
        while let Some(item) = buffer.try_pop() {
            items.push(item);
        }
        items
    }

    ktest! {
        /// `try_pop` of an empty buffer gives `None`.
        fn try_pop_of_an_empty_buffer_gives_none() {
            let buffer: Buffer<u32, 4> = Buffer::new();
            check!(buffer.try_pop().is_none());
        }

        /// Items leave in the order they enter.
        fn items_leave_in_order() {
            let buffer: Buffer<u32, 4> = Buffer::new();
            for i in 0..3 {
                buffer.push(i);
            }
            check_eq!(drain(&buffer), [0, 1, 2]);
            check!(buffer.try_pop().is_none());
        }

        /// A full buffer drops the oldest item on `push`.
        fn full_buffer_drops_the_oldest_item() {
            let buffer: Buffer<u32, 4> = Buffer::new();
            for i in 0..6 {
                buffer.push(i);
            }
            check_eq!(drain(&buffer), [2, 3, 4, 5]);
        }

        /// A buffer of size 1 keeps only the newest item.
        fn buffer_of_size_one_keeps_the_newest_item() {
            let buffer: Buffer<u32, 1> = Buffer::new();
            buffer.push(1);
            buffer.push(2);
            check_eq!(drain(&buffer), [2]);
            buffer.push(3);
            check_eq!(buffer.try_pop(), Some(3));
        }

        /// 1000 push and pop pairs keep the order across the wrap of the ring.
        fn many_push_pop_pairs_keep_the_order() {
            let buffer: Buffer<u32, 4> = Buffer::new();
            buffer.push(0);
            for i in 1..=1000 {
                buffer.push(i);
                check_eq!(buffer.try_pop(), Some(i - 1));
            }
            check_eq!(drain(&buffer), [1000]);
        }

        /// `pop` blocks until another thread pushes an item.
        fn pop_blocks_until_a_push() {
            let buffer: Arc<Buffer<u32, 4>> = Arc::new(Buffer::new());
            let got = Arc::new(AtomicUsize::new(0));
            let (b, g) = (Arc::clone(&buffer), Arc::clone(&got));
            spawn_thread(move || {
                g.store(b.pop() as usize + 1, Ordering::SeqCst);
            });

            let waiting = wait_until(|| waiters(&buffer.readable) == 1);
            let early = got.load(Ordering::SeqCst);
            buffer.push(7);
            let done = wait_until(|| got.load(Ordering::SeqCst) != 0);

            check!(waiting, "the reader did not wait");
            check_eq!(early, 0);
            check!(done, "the reader did not wake");
            check_eq!(got.load(Ordering::SeqCst), 8);
        }

        /// The drop of a buffer drops each item in it.
        fn drop_of_the_buffer_drops_each_item() {
            let item = Arc::new(());
            let buffer: Buffer<Arc<()>, 4> = Buffer::new();
            for _ in 0..3 {
                buffer.push(Arc::clone(&item));
            }
            drop(buffer);
            check_eq!(Arc::strong_count(&item), 1);
        }

        /// An item that `push` overwrites is dropped.
        /// Today `set_index` writes over the old item and leaks it.
        #[known_bug(unfiled)]
        fn overwritten_item_is_dropped() {
            let oldest = Arc::new(());
            let buffer: Buffer<Arc<()>, 4> = Buffer::new();
            buffer.push(Arc::clone(&oldest));
            for _ in 0..4 {
                buffer.push(Arc::new(()));
            }
            let count = Arc::strong_count(&oldest);
            drop(buffer);
            check_eq!(count, 1);
        }
    }
}

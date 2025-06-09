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
        let len = self.len.load(Ordering::Acquire) + 1;
        let tail = self.tail.load(Ordering::Relaxed);
        let next = (tail + 1) % S;

        let head = self.head.load(Ordering::Acquire);
        if tail == head { // Push the head if overflow
            self.head.store((head + 1) % S, Ordering::Release);
        }

        self.set_index(tail, value);
        self.tail.store(next, Ordering::Release);

        // Once len hits the size of the buffer, dont increase it further cause
        // at this poitn we're overwriting, not adding
        if len <= S {
            self.len.store(len, Ordering::Release);
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

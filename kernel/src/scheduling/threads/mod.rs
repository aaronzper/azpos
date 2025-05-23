use alloc::collections::btree_map::BTreeMap;
use lazy_static::lazy_static;
use spin::RwLock;

mod thread;
pub use thread::{ThreadID, Thread};

/// Contains all threads on the system, and some metadata
pub struct ThreadTable {
    threads: BTreeMap<ThreadID, Thread>,
    next_id: ThreadID,
}

impl ThreadTable {
    pub fn new() -> ThreadTable {
        ThreadTable {
            threads: BTreeMap::new(),
            next_id: 0,
        }
    }

    /// Adds a new thread to the table and returns its given ID
    pub fn add_thread(&mut self, thread: Thread) -> ThreadID {
        let first_id = self.next_id;
        loop { // TODO: Optimize to not be O(n)
            let id = self.next_id;
            self.next_id += 1;
            if !self.threads.contains_key(&id) {
                self.threads.insert(id, thread);
                return id;
            }

            // We've gone through every thread ID and gotten back to the start
            if self.next_id == first_id {
                panic!("Out of Thread IDs!");
            }
        }
    }

    /// Returns a ref to a thread by ID, if it exists
    pub fn get_thread(&self, id: ThreadID) -> Option<&Thread> {
        self.threads.get(&id)
    }
    ///
    /// Returns a mutable refernce to a thread by ID, if it exists
    pub fn get_thread_mut(&mut self, id: ThreadID) -> Option<&mut Thread> {
        self.threads.get_mut(&id)
    }
}

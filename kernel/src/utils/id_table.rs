use core::ops::AddAssign;

use alloc::collections::btree_map::BTreeMap;
use num_traits::Unsigned;

/// See module documentation
pub struct IDTable<I: Unsigned, V> {
    entries: BTreeMap<I, V>,
    next_id: I,
}

impl<I: Unsigned + Copy + AddAssign + Ord, V> IDTable<I, V> {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            next_id: I::zero(),
        }
    }

    /// Adds a new entry to the table and returns its given ID
    ///
    /// Panics if out of IDs
    pub fn add_entry(&mut self, entry: V) -> I {
        let first_id = self.next_id;
        loop { // TODO: Optimize to not be O(n)
            let id = self.next_id;
            self.next_id += I::one();
            if !self.entries.contains_key(&id) {
                self.entries.insert(id, entry);
                return id;
            }

            // We've gone through every ID and gotten back to the start
            if self.next_id == first_id {
                panic!("Out of IDs!");
            }
        }
    }

    /// Removes the given entry by ID, if it exists, and returns that entry
    pub fn remove_entry(&mut self, entry: I) -> Option<V> {
        self.entries.remove(&entry)
    }

    /// Returns a ref to a entry by ID, if it exists
    pub fn get_entry(&self, id: I) -> Option<&V> {
        self.entries.get(&id)
    }

    /// Returns a mutable refernce to an entry by ID, if it exists
    pub fn get_entry_mut(&mut self, id: I) -> Option<&mut V> {
        self.entries.get_mut(&id)
    }
}

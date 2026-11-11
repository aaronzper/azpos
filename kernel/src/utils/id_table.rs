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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktest::*;

    ktest! {
        /// A new table gives the IDs 0, 1, 2 in order.
        fn ids_count_up_from_zero() {
            let mut table: IDTable<u32, char> = IDTable::new();
            check_eq!(table.add_entry('a'), 0);
            check_eq!(table.add_entry('b'), 1);
            check_eq!(table.add_entry('c'), 2);
        }

        fn get_returns_the_entry_of_an_id() {
            let mut table: IDTable<u32, char> = IDTable::new();
            let a = table.add_entry('a');
            let b = table.add_entry('b');

            check_eq!(table.get_entry(a), Some(&'a'));
            check_eq!(table.get_entry(b), Some(&'b'));
            check_eq!(table.get_entry(99), None);

            *table.get_entry_mut(a).or_fail("get_entry_mut")? = 'z';
            check_eq!(table.get_entry(a), Some(&'z'));
        }

        fn remove_returns_the_entry_one_time() {
            let mut table: IDTable<u32, char> = IDTable::new();
            let a = table.add_entry('a');

            check_eq!(table.remove_entry(a), Some('a'));
            check_eq!(table.remove_entry(a), None);
            check_eq!(table.get_entry(a), None);
        }

        /// The table does not give out an ID again directly after its
        /// removal. A stale ID then does not refer to a new entry.
        fn a_removed_id_is_not_used_again_at_once() {
            let mut table: IDTable<u32, char> = IDTable::new();
            let a = table.add_entry('a');
            table.remove_entry(a);

            check_ne!(table.add_entry('b'), a);
        }

        /// The ID counter wraps to 0 after the last ID and finds a free ID.
        /// Today `next_id += 1` overflows, which panics in a debug build.
        #[known_bug(unfiled, crash)]
        fn the_id_counter_wraps_to_a_free_id() {
            let mut table: IDTable<u8, u8> = IDTable::new();
            for i in 0..=254 {
                check_eq!(table.add_entry(i), i);
            }
            table.remove_entry(3);

            // ID 255 is free, then the counter wraps and finds ID 3.
            check_eq!(table.add_entry(0), 255);
            check_eq!(table.add_entry(0), 3);
        }
    }
}

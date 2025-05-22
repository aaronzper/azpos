use alloc::collections::btree_map::BTreeMap;
use lazy_static::lazy_static;
use spin::RwLock;

mod state;
mod thread;
pub use thread::{ThreadID, Thread};

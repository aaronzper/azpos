#![no_std]
#![no_main]

pub use libsci;

extern crate alloc;
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

/// Syscalls
pub mod syscalls;
/// Userspace runtime (entrypoint, panic, etc)
pub mod runtime;

#![no_std]

/// Functions for actually making a syscall
pub mod syscall;

#[non_exhaustive]
#[repr(u64)]
#[derive(Debug, Copy, Clone)]
/// The code for each syscall
pub enum Syscall {
    /// Testing call to just make sure it works
    TestPing,
}

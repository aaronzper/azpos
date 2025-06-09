#![no_std]

#[non_exhaustive]
#[repr(u64)]
#[derive(Debug, Copy, Clone)]
/// The code for each syscall
pub enum Syscall {
    /// Yields control to the scheuler
    Yield,
    /// Testing call to just make sure it works
    TestPing,
    /// Prints the string pointer at arg1 with len arg2
    Print,
}

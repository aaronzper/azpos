#![no_std]

pub use postcard;

extern crate alloc;

/// Definitions for a `Resource`
pub mod resources;
/// Device-related structs and definitions
pub mod devices;

#[non_exhaustive]
#[repr(u64)]
#[derive(Debug, Copy, Clone)]
/// azpOS syscalls.
///
/// Syscalls are invoked via the `syscall` instruction, but are generally invoked
/// via `libsystem` instead of manually. Return value is given by `rax`.
/// Arguments are passed as follows:
///
/// - `rdi` - Syscall number
/// - `rsi` - First syscall argument
/// - `rdx` - Second syscall argument
/// - `r8`  - Third syscall argument
/// - *Note that `rcx` is used by the `syscall` instruction and as such is not
///   used for arguments.*
///
/// For syscalls that take less than three arguments, the remaining arguments
/// are reserved and should be set to zero. Other than `rax`, the values of the
/// other registers are undefined after a syscall (except for `rsp`, `rip`, and
/// `rflags`) so save them before calling.
///
/// Discriminants are explicit and stable — append new variants at the end.
pub enum Syscall {
    /// Yields control to the scheduler
    ///
    /// Takes no arguments
    ///
    /// Returns nothing
    Yield = 0,

    /// Temporary test syscall that returns a resource that, when written to,
    /// prints to the kernel log
    ///
    /// Takes no arguments
    ///
    /// Returns: A resource ID to the logger resource
    GetLogger = 1,

    /// Read from a resource
    ///
    /// Arguments:
    /// 1. The resource ID
    /// 2. Pointer to the buffer to read to
    /// 3. Length of the buffer in bytes
    ///
    /// Returns:
    /// - The number of bytes read
    /// - OR a negative error code
    Read = 2,

    /// Write to a resource
    ///
    /// Arguments:
    /// 1. The resource ID
    /// 2. Pointer to the buffer to write from
    /// 3. Length of the buffer in bytes
    ///
    /// Returns:
    /// - The number of bytes written
    /// - OR a negative error code
    Write = 3,

    /// Closes a resource, removing it from the process and cleaning up whatever
    /// it holds
    ///
    /// Arguments:
    /// 1. The resource ID
    ///
    /// Returns nothing
    Close = 4,

    /// Sets the seek head of a resource to the given offset
    ///
    /// Arguments:
    /// 1. The resource ID
    /// 2. Offset from the beginning of the resource to set the seek head to
    ///
    /// Returns:
    /// - 0 on success
    /// - OR a negative error code
    Seek = 5,

    /// Returns a Resource that, when read, provides a serialized list of the
    /// devices on the system. TODO: Document further
    ///
    /// No arguments
    ///
    /// Returns the RID of that resource
    ListDevices = 6,
}

impl TryFrom<u64> for Syscall {
    type Error = ();

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Syscall::Yield),
            1 => Ok(Syscall::GetLogger),
            2 => Ok(Syscall::Read),
            3 => Ok(Syscall::Write),
            4 => Ok(Syscall::Close),
            5 => Ok(Syscall::Seek),
            6 => Ok(Syscall::ListDevices),
            _ => Err(()),
        }
    }
}

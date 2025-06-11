#![no_std]
pub use postcard;

extern crate alloc;

/// Definitions for a `Resource`
pub mod resources;

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
pub enum Syscall {
    /// Yields control to the scheduler
    ///
    /// Takes no arguments
    ///
    /// Returns nothing
    Yield,
    
    /// Temporary test syscall that returns a resource that, when written to,
    /// prints to the kernel log
    /// 
    /// Takes no arguments
    ///
    /// Returns: A resource ID to the logger resource
    GetLogger,

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
    Read,
    
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
    Write,
    
    /// Closes a resource, removing it from the process and cleaning up whatever
    /// it holds
    ///
    /// Arguments:
    /// 1. The resource ID
    ///
    /// Returns nothing
    Close,
    
    /// Sets the seek head of a resource to the givern offset
    ///
    /// Arguments:
    /// 1. The resource ID
    /// 2. Offset from the beginning of the resource to set the seek head to
    ///
    /// Returns:
    /// - 0 on success
    /// - OR a negative error code
    Seek,
}

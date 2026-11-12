//! Cases that give the kernel a bad buffer. The kernel must reject each one
//! with an error.

use libsystem::syscalls::{raw_syscall, sys_close, sys_get_logger, sys_list_devices};
use libtestkit::{cases, check};

use crate::{abi::{READ, WRITE}, devices::read_to_end};

/// A user address with no page
const UNMAPPED_USER: u64 = 0x1000;
/// A kernel address with no page
const UNMAPPED_KERNEL: u64 = 0xFFFF_F000_0000_0000;
/// The first address after the user half of the address space
const USER_END_ADDR: u64 = 0x0000_8000_0000_0000;

/// Data in a read-only section of the program
static READ_ONLY: [u8; 16] = [0x5A; 16];

/// Reads from a new blob into the raw buffer and gives the raw result
fn raw_read(ptr: u64, len: u64) -> i64 {
    let blob = sys_list_devices();
    let ret = unsafe { raw_syscall(READ, blob as u64, ptr, len) };
    let _ = sys_close(blob);
    ret
}

/// Writes from the raw buffer to a new logger and gives the raw result
fn raw_write(ptr: u64, len: u64) -> i64 {
    let logger = sys_get_logger();
    let ret = unsafe { raw_syscall(WRITE, logger as u64, ptr, len) };
    let _ = sys_close(logger);
    ret
}

cases! {
    #[known_bug(26, crash)]
    fn read_into_an_unmapped_user_address_gives_an_error() {
        let ret = raw_read(UNMAPPED_USER, 16);
        check!(ret < 0, "read gave {}", ret);
    }

    #[known_bug(26, crash)]
    fn read_into_a_kernel_address_gives_an_error() {
        let ret = raw_read(UNMAPPED_KERNEL, 16);
        check!(ret < 0, "read gave {}", ret);
    }

    #[known_bug(26, crash)]
    fn write_from_an_unmapped_user_address_gives_an_error() {
        let ret = raw_write(UNMAPPED_USER, 16);
        check!(ret < 0, "write gave {}", ret);
    }

    #[known_bug(26, crash)]
    fn write_from_a_kernel_address_gives_an_error() {
        let ret = raw_write(UNMAPPED_KERNEL, 16);
        check!(ret < 0, "write gave {}", ret);
    }

    /// The buffer is larger than the data, so today the read does not
    /// overflow it.
    #[known_bug(26)]
    fn read_with_the_length_usize_max_gives_an_error() {
        let blob = sys_list_devices();
        let size = read_to_end(blob)?.len();
        let _ = sys_close(blob);

        let mut buf = [0u8; 4096];
        check!(size < buf.len(), "the blob has {} bytes", size);
        let ret = raw_read(buf.as_mut_ptr() as u64, usize::MAX as u64);
        check!(ret < 0, "read gave {}", ret);
    }

    /// Today the kernel gives -3 by chance.
    /// It finds a byte that is not UTF-8 on the stack after `buf`.
    fn write_with_the_length_1_shl_40_gives_an_error() {
        let buf = [b'a'; 64];
        let ret = raw_write(buf.as_ptr() as u64, 1 << 40);
        check!(ret < 0, "write gave {}", ret);
    }

    #[known_bug(26, crash)]
    fn read_across_the_end_of_user_memory_gives_an_error() {
        let ret = raw_read(USER_END_ADDR - 8, 16);
        check!(ret < 0, "read gave {}", ret);
    }

    /// A non-canonical address is in neither half of the address space.
    #[known_bug(26, crash)]
    fn read_into_a_non_canonical_address_gives_an_error() {
        let ret = raw_read(USER_END_ADDR, 16);
        check!(ret < 0, "read gave {}", ret);
    }

    /// A length of 0 needs no memory, so a null pointer is not an error.
    /// An error is also correct.
    /// Today `slice::from_raw_parts` in the kernel panics on the null pointer.
    #[known_bug(26, crash)]
    fn write_of_zero_bytes_from_a_null_pointer_does_not_panic_the_kernel() {
        let ret = raw_write(0, 0);
        check!(ret == 0 || ret < 0, "write gave {}", ret);
    }

    /// Today `slice::from_raw_parts_mut` in the kernel panics on the null pointer.
    #[known_bug(26, crash)]
    fn read_of_zero_bytes_into_a_null_pointer_does_not_panic_the_kernel() {
        let ret = raw_read(0, 0);
        check!(ret == 0 || ret < 0, "read gave {}", ret);
    }

    /// Today the kernel writes into the read-only data and gives 16.
    #[known_bug(26)]
    fn read_into_read_only_memory_gives_an_error() {
        let ret = raw_read(READ_ONLY.as_ptr() as u64, 16);
        let first = unsafe { core::ptr::read_volatile(READ_ONLY.as_ptr()) };
        check!(ret < 0, "read gave {} first byte {:#x}", ret, first);
    }
}

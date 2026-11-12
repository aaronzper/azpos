//! Cases that make syscalls with raw numbers and raw arguments.

use core::arch::asm;

use libsystem::syscalls::{raw_syscall, sys_close, sys_get_logger};
use libtestkit::{cases, check, check_eq};

// The numbers of the syscalls, in the order of `libsci::Syscall`.
pub const YIELD: u64 = 0;
pub const GET_LOGGER: u64 = 1;
pub const READ: u64 = 2;
pub const WRITE: u64 = 3;
pub const CLOSE: u64 = 4;
pub const SEEK: u64 = 5;
pub const LIST_DEVICES: u64 = 6;

/// The first address of the kernel half of the address space
const KERNEL_START: u64 = 0xFFFF_8000_0000_0000;

/// Makes a syscall and gives the values of `rdi`, `rsi`, `rdx`, `r8`, `r9`
/// and `r10` after it.
fn syscall_scratch(number: u64, arg1: u64) -> [u64; 6] {
    let (rdi, rsi, rdx, r8, r9, r10): (u64, u64, u64, u64, u64, u64);
    // The kernel entry writes the user stack pointer into `rbx`.
    // `asm!` cannot name `rbx`, so the stub saves it on the stack.
    unsafe {
        asm!(
            "push rbx",
            "syscall",
            "pop rbx",
            inout("rdi") number => rdi,
            inout("rsi") arg1 => rsi,
            inout("rdx") 0u64 => rdx,
            inout("r8") 0u64 => r8,
            inout("r9") 0u64 => r9,
            inout("r10") 0u64 => r10,
            clobber_abi("C"),
        );
    }
    [rdi, rsi, rdx, r8, r9, r10]
}

cases! {
    fn yield_returns_zero() {
        check_eq!(unsafe { raw_syscall(YIELD, 0, 0, 0) }, 0);
    }

    /// The kernel ignores the arguments that a syscall does not use.
    fn yield_with_values_in_the_unused_arguments_returns_zero() {
        let ret = unsafe { raw_syscall(YIELD, u64::MAX, 0xDEAD_BEEF, KERNEL_START) };
        check_eq!(ret, 0);
    }

    fn get_logger_gives_a_resource_id_that_is_not_negative() {
        let rid = unsafe { raw_syscall(GET_LOGGER, 0, 0, 0) };
        check!(rid >= 0, "get_logger gave {}", rid);
        check!(sys_close(rid as u32).is_ok());
    }

    /// The kernel gives -3 (`InvalidInput`) for text that is not UTF-8.
    /// The raw call does not use `rax_to_result`, so #34 does not apply.
    fn write_of_invalid_utf8_gives_minus_three() {
        let logger = sys_get_logger();
        let bytes = [0xFFu8, 0xFE];
        let ret = unsafe { raw_syscall(WRITE, logger as u64, bytes.as_ptr() as u64, 2) };
        check_eq!(ret, -3);
        check!(sys_close(logger).is_ok());
    }

    /// The design wants a negative value for an unknown number.
    /// Today the kernel converts the number to `Syscall`, which is undefined behavior.
    #[known_bug(28, crash)]
    fn the_first_number_after_the_last_syscall_gives_a_negative_value() {
        let ret = unsafe { raw_syscall(LIST_DEVICES + 1, 0, 0, 0) };
        check!(ret < 0, "number {} gave {}", LIST_DEVICES + 1, ret);
    }

    #[known_bug(28, crash)]
    fn the_number_0xffff_gives_a_negative_value() {
        let ret = unsafe { raw_syscall(0xFFFF, 0, 0, 0) };
        check!(ret < 0, "number 0xFFFF gave {}", ret);
    }

    #[known_bug(28, crash)]
    fn the_number_u64_max_gives_a_negative_value() {
        let ret = unsafe { raw_syscall(u64::MAX, 0, 0, 0) };
        check!(ret < 0, "number u64::MAX gave {}", ret);
    }

    /// No scratch register holds a kernel address after a syscall.
    /// Today `rdi` holds a kernel stack address after `yield`.
    #[known_bug(36)]
    fn no_scratch_register_holds_a_kernel_address_after_a_syscall() {
        for (number, arg1) in [(YIELD, 0), (SEEK, 0xFFFF_FF00), (CLOSE, 0xFFFF_FF00)] {
            let regs = syscall_scratch(number, arg1);
            for (i, &value) in regs.iter().enumerate() {
                check!(value < KERNEL_START, "syscall {} register {} holds {:#x}", number, i, value);
            }
        }
    }
}

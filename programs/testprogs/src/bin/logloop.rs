//! Gets a logger, then writes "x" to it in a loop. It never exits.

#![no_std]
#![no_main]

use libsystem::syscalls::{sys_get_logger, sys_write, sys_yield};

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    let logger = sys_get_logger();
    loop {
        let _ = sys_write(logger, b"x");
        sys_yield();
    }
}

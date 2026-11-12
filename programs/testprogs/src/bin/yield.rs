//! Gives its time slice away in a loop. It never exits.

#![no_std]
#![no_main]

use libsystem::syscalls::sys_yield;

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    loop {
        sys_yield();
    }
}

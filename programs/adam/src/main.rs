#![no_std]
#![no_main]

use libsystem::syscalls::print;

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    let mut i = 1;
    loop {
        if i % 10000000 == 0 {
            panic!("Hello world I am a process!");
            i = 0;
        } 

        i += 1;
    }
}

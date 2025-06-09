#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

use libsyscall::syscall::make_syscall;

#[unsafe(no_mangle)]
pub fn _start() -> ! {
    let msg = "Hello world i am a process!";

    let mut i = 1;
    loop {
        if i % 10000000 == 0 {
            let ptr = msg.as_ptr() as u64;
            let len = msg.len() as u64;
            make_syscall(libsyscall::Syscall::Print, ptr, len);
            i = 0;
        } 
        
        i += 1;
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    loop {}
}

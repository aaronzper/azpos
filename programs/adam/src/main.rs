#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

use libsyscall::syscall::make_syscall;

#[unsafe(no_mangle)]
pub fn _start() -> ! {
    let mut i = 1;
    loop {
        if i % 10000000 == 0 {
            make_syscall(libsyscall::Syscall::TestPing);
            i = 0;
        } 
        
        i += 1;
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    loop {}
}

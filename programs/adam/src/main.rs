#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

use libsyscall::syscall::make_syscall;

#[unsafe(no_mangle)]
pub fn _start() -> ! {
    let mut i = 0;
    loop {
        make_syscall(libsyscall::Syscall::TestPing);

        if i % 10 == 0 {
            make_syscall(libsyscall::Syscall::Yield);
            i = 0;
        } 
        
        i += 1;
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    loop {}
}

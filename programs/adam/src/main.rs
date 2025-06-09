#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

use libsyscall::syscall::make_syscall;

#[unsafe(no_mangle)]
pub fn _start() -> ! {
    make_syscall(libsyscall::Syscall::TestPing);
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    loop {}
}

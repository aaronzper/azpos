#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

#[unsafe(no_mangle)]
pub fn _start() -> ! {
    // Force a fault since this instruction isnt allowed in user mode (uncomment
    // it to demonstrate)
    //unsafe { asm!("cli") };
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    loop {}
}

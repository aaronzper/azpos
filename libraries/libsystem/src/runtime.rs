use alloc::format;

use crate::syscalls::print;
use core::{panic::PanicInfo};

unsafe extern "C" { 
    fn main();
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    unsafe { main(); }

    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    print(&format!("!! PANIC !! {info:#?}"));
    loop {}
}

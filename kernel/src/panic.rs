use core::panic::PanicInfo;

use crate::{logger::logger_initialized, println};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if logger_initialized() {
        println!("!!! KERNEL PANIC !!!");
        println!("{}", info);
    }

    loop {}
}

use core::panic::PanicInfo;
use crate::{println, terminal::global::global_terminal_initialized};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if global_terminal_initialized() {
        println!("!!! KERNEL PANIC !!!");
        println!("{}", info);
    }

    loop {}
}

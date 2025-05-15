use core::panic::PanicInfo;

use crate::{devices::fb::RgbPixel, logger::{logger_initialized, set_fg_color}, println};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if logger_initialized() {
        set_fg_color(RgbPixel { red: 0xFF, green: 0, blue: 0 });
    }

    println!("\n!!! KERNEL PANIC !!!");
    println!("{}", info);

    loop {}
}

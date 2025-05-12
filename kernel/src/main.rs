#![no_std]
#![no_main]

use core::fmt::Write;

use bootloader_api::{info::Optional, BootInfo};
use devices::fb::{FbTerminal, Framebuffer};
use logger::set_logger;

mod panic;
mod logger;
mod devices;

bootloader_api::entry_point!(kmain);

fn kmain(boot_info: &'static mut BootInfo) -> ! {
    let fb_raw = match &mut boot_info.framebuffer {
        Optional::Some(x) => x,
        Optional::None => panic!("No framebuffer!"),
    };

    let fb = Framebuffer::new(fb_raw);
    let t = FbTerminal::new(fb);
    set_logger(t);

    println!("Hello world!");
    println!("{:?}", boot_info.memory_regions);

    panic!("End of kmain");
}

#![no_std]
#![no_main]

use bootloader_api::BootInfo;

mod panic;

bootloader_api::entry_point!(kmain);

fn kmain(boot_info: &'static mut BootInfo) -> ! {
    panic!("Ahh!");
}

#![no_std]
#![no_main]

use bootloader_api::{config::Mapping, info::Optional, BootInfo, BootloaderConfig};
use devices::fb::{FbTerminal, Framebuffer};
use logger::set_logger;

mod panic;
mod logger;
mod devices;

const BOOTCONFIG: BootloaderConfig = {
    let mut conf = BootloaderConfig::new_default();
    conf.mappings.physical_memory = Some(Mapping::Dynamic);
    conf
};
bootloader_api::entry_point!(kmain, config = &BOOTCONFIG);

fn kmain(boot_info: &'static mut BootInfo) -> ! {
    let fb_raw = match &mut boot_info.framebuffer {
        Optional::Some(x) => x,
        Optional::None => panic!("No framebuffer!"),
    };

    let fb_addr = fb_raw.buffer().as_ptr();

    let fb = Framebuffer::new(fb_raw);
    let t = FbTerminal::new(fb);
    set_logger(t);

    println!("Hello world!");
    println!("Kernel Start:\t{:#X}", boot_info.kernel_image_offset);
    println!("Kernel End:\t{:#X}", boot_info.kernel_image_offset + boot_info.kernel_len);
    println!("Phys Mapping:\t{:#X}", boot_info.physical_memory_offset.take().unwrap());
    println!("Fb Mapping:\t{:?}", fb_addr);
    println!("Memory Regions:\n{:?}", boot_info.memory_regions);

    for i in 0..=u8::MAX as u16 * 3 {
        print!("R");
        for _ in 0..=u8::MAX as u32 * 2000 {}
    }

    panic!("End of kmain");
}

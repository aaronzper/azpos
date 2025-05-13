#![no_std]
#![no_main]

use bootloader_api::{config::Mapping, info::Optional, BootInfo, BootloaderConfig};
use devices::fb::{FbTerminal, Framebuffer};
use logger::set_logger;
use memory::{init_memory, KERNEL_START_ADDR};

#[macro_use]
mod logger;
mod panic;
mod devices;
mod memory;

const BOOTCONFIG: BootloaderConfig = {
    let mut conf = BootloaderConfig::new_default();
    conf.mappings.dynamic_range_start = Some(KERNEL_START_ADDR);
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

    let pmap = boot_info.physical_memory_offset.take().unwrap();
    let k_end_va = boot_info.kernel_image_offset + boot_info.kernel_len;
    init_memory(pmap, &boot_info.memory_regions, k_end_va);

    panic!("End of kmain");
}

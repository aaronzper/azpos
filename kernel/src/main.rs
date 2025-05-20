#![no_std]
#![no_main]
#![feature(new_zeroed_alloc)]

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use bootloader_api::{config::Mapping, info::Optional, BootInfo, BootloaderConfig};
use devices::fb::{FbTerminal, Framebuffer};
use interrupts::init_interrupts;
use logger::set_logger;
use memory::{get_heap_size, init_memory, KERNEL_START_ADDR};

#[macro_use]
/// Global kernel logger
mod logger;
/// Kernel panic functionality
mod panic;
/// Device drivers
mod devices;
/// Memory subsystem (paging, the heap, etc)
mod memory;
/// CPU interrupt subsystem (faults, hardware interrupts, & syscalls)
mod interrupts;

const BOOTCONFIG: BootloaderConfig = {
    let mut conf = BootloaderConfig::new_default();
    conf.mappings.dynamic_range_start = Some(KERNEL_START_ADDR);
    conf.mappings.physical_memory = Some(Mapping::Dynamic);
    conf
};
bootloader_api::entry_point!(kmain, config = &BOOTCONFIG);

/// Initializes the kernel and its subsystems
fn kmain(boot_info: &'static mut BootInfo) -> ! {
    let pmap = boot_info.physical_memory_offset.take().unwrap();
    init_memory(pmap, &boot_info.memory_regions);

    let fb = match &mut boot_info.framebuffer {
        Optional::Some(x) => x,
        Optional::None => panic!("No framebuffer!"),
    };
    let fb_info = fb.info().clone();
    let fb_buf = fb.buffer_mut();
    let fb = Framebuffer::new(fb_buf, fb_info);
    let t = FbTerminal::new(fb);
    set_logger(t);

    init_interrupts();

    panic!("End of kmain");
}

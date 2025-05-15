#![no_std]
#![no_main]
#![feature(new_zeroed_alloc)]

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use bootloader_api::{config::Mapping, info::Optional, BootInfo, BootloaderConfig};
use devices::fb::{FbTerminal, Framebuffer};
use logger::set_logger;
use memory::{get_heap_size, init_memory, KERNEL_START_ADDR};

#[macro_use]
/// Global Kernel logger
mod logger;
/// Kernel panic functionality
mod panic;
/// Device drivers
mod devices;
/// Memory subsystem (paging, the heap, etc)
mod memory;

const BOOTCONFIG: BootloaderConfig = {
    let mut conf = BootloaderConfig::new_default();
    conf.mappings.dynamic_range_start = Some(KERNEL_START_ADDR);
    conf.mappings.physical_memory = Some(Mapping::Dynamic);
    conf
};
bootloader_api::entry_point!(kmain, config = &BOOTCONFIG);

/// Initializes the kernel and its subsystems
fn kmain(boot_info: &'static mut BootInfo) -> ! {
    let fb = match &mut boot_info.framebuffer {
        Optional::Some(x) => x,
        Optional::None => panic!("No framebuffer!"),
    };
    let fb_info = fb.info().clone();
    let fb_buf = fb.buffer_mut();
    let fb_end =
        (fb_buf.as_ptr() as u64) + fb_buf.len() as u64;

    let pmap = boot_info.physical_memory_offset.take().unwrap();
    let pmap_end = pmap + boot_info.memory_regions.last().unwrap().end;

    let kernel_end = boot_info.kernel_image_offset + boot_info.kernel_len;

    // The bootloader maps the kernel, framebuffer, and all of physical memory
    // into the higher-half virtual address space. The largest of the ends of
    // these three mappings indicates the virtual address space we can
    // start using.
    let usable_start =
        [fb_end, pmap_end, kernel_end].into_iter().max().unwrap();
    init_memory(pmap, &boot_info.memory_regions, usable_start);

    let fb = Framebuffer::new(fb_buf, fb_info);
    let t = FbTerminal::new(fb);
    set_logger(t);
    println!("Hello world!");

    let mut v: Vec<Box<u128>> = Vec::new();
    for i in 0..5000 {
        let b = Box::new(i);
        v.push(b);
    }

    println!("\nBoxes:");
    for b in &v {
        print!("{}\t", b);
    }
    println!("");

    println!("Before drop heap size: {} kb", get_heap_size() / 1024);
    drop(v);
    println!("After drop heap size:  {} kb", get_heap_size() / 1024);

    panic!("End of kmain");
}

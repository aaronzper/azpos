#![no_std]
#![no_main]

use bootloader_api::{info::Optional, BootInfo};

mod panic;

bootloader_api::entry_point!(kmain);

fn kmain(boot_info: &'static mut BootInfo) -> ! {
    let fb = match &mut boot_info.framebuffer {
        Optional::Some(x) => x,
        Optional::None => panic!("No framebuffer!"),
    };

    let h = fb.info().height;
    let w = fb.info().width;
    let bpp = fb.info().bytes_per_pixel;
    let stride = fb.info().stride;

    loop {
        for amt in 0..u8::MAX {
            for r in 0..h {
                for c in 0..w {
                    let px_index = r * stride + c;
                    let b_index = px_index * bpp;

                    fb.buffer_mut()[b_index] = amt;
                }
            }
        }
    }
}

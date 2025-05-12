#![no_std]
#![no_main]

use bootloader_api::{info::Optional, BootInfo};
use devices::fb::{Framebuffer, RgbPixel};

mod panic;
mod devices;

bootloader_api::entry_point!(kmain);

fn kmain(boot_info: &'static mut BootInfo) -> ! {
    let fb_raw = match &mut boot_info.framebuffer {
        Optional::Some(x) => x,
        Optional::None => panic!("No framebuffer!"),
    };

    let mut fb = Framebuffer::new(fb_raw);

    let colors = [
        RgbPixel { red: 0xFF, green: 0, blue: 0 },
        RgbPixel { red: 0, green: 0xFF, blue: 0 },
        RgbPixel { red: 0, green: 0, blue: 0xFF },
        RgbPixel { red: 0xFF, green: 0xFF, blue: 0xFF },
    ];

    loop {
        for color in colors {
            for i in 0..=u8::MAX {
                let w = (i as f32/u8::MAX as f32) * fb.get_width() as f32;

                for r in (fb.get_height() - 200)..fb.get_height() {
                    for c in 0..w as usize {
                        fb.draw_pixel(c, r, color);
                    }
                }
            }
        }
    }
}

use bootloader_api::info::{FrameBuffer, PixelFormat};

#[derive(Copy, Clone)]
pub struct RgbPixel {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl RgbPixel {
    pub fn average(&self) -> u8 {
        (self.red + self.green + self.blue) / 3
    }
}

pub struct Framebuffer {
    height: usize,
    width: usize,
    bytes_per_pixel: usize,
    stride: usize,
    format: PixelFormat,

    buffer: &'static mut [u8],
}

impl Framebuffer {
    pub fn new(fb: &'static mut FrameBuffer) -> Framebuffer {
        Framebuffer {
            height: fb.info().height,
            width: fb.info().width,
            bytes_per_pixel: fb.info().bytes_per_pixel,
            stride: fb.info().stride,
            format: fb.info().pixel_format,
            buffer: fb.buffer_mut(),
        }
    }

    pub fn get_height(&self) -> usize {
        self.height
    }

    pub fn get_width(&self) -> usize {
        self.width
    }

    pub fn is_grayscale(&self) -> bool {
        self.format == PixelFormat::U8
    }

    pub fn draw_pixel(&mut self, x: usize, y: usize, pixel: RgbPixel) {
        let px_index = y * self.stride + x;
        let b_index = px_index * self.bytes_per_pixel;

        match self.format {
            PixelFormat::U8 => self.buffer[b_index] = pixel.average(),

            PixelFormat::Rgb => {
                self.buffer[b_index] = pixel.red;
                self.buffer[b_index + 1] = pixel.green;
                self.buffer[b_index + 2] = pixel.blue;
            }

            PixelFormat::Bgr => {
                self.buffer[b_index] = pixel.blue;
                self.buffer[b_index + 1] = pixel.green;
                self.buffer[b_index + 2] = pixel.red;
            },

            _ => unimplemented!("Unsupported pixel format"),
        }
    }

    pub fn clear(&mut self) {
        let black = RgbPixel { red: 0, green: 0, blue: 0 };

        for r in 0..self.height {
            for c in 0..self.width {
                self.draw_pixel(c, r, black);
            }
        }
    }
}

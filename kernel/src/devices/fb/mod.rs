use alloc::boxed::Box;
use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};

mod terminal;
pub use terminal::FbTerminal;

/// A single RGB pixel
#[derive(Copy, Clone)]
pub struct RgbPixel {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl RgbPixel {
    /// The average value of each pixel (used for grayscale displays)
    pub fn average(&self) -> u8 {
        (self.red + self.green + self.blue) / 3
    }
}

/// A double-buffered framebuffer and its metadata
pub struct Framebuffer {
    height: usize,
    width: usize,
    bytes_per_pixel: usize,
    stride: usize,
    format: PixelFormat,

    buffer: &'static mut [u8],
    back_buffer: Box<[u8]>,
}

impl Framebuffer {
    /// Creates a double-buffered framebuffer, given information on one from 
    /// the bootloader
    pub fn new(fb: &'static mut [u8], info: FrameBufferInfo) -> Framebuffer {
        let height = info.height;
        let width = info.width;
        let bytes_per_pixel = info.bytes_per_pixel;
        let stride = info.stride;
        let format = info.pixel_format;
        let back_buffer = unsafe { // Just u8s, this is safe
            Box::new_zeroed_slice(info.byte_len).assume_init()
        };
        let buffer = fb;

        Framebuffer {
            height,
            width,
            bytes_per_pixel,
            stride,
            format,
            back_buffer,
            buffer,
        }
    }

    /// The height, in pixels, of the display
    pub fn get_height(&self) -> usize {
        self.height
    }

    /// The width, in pixels, of the display
    pub fn get_width(&self) -> usize {
        self.width
    }

    /// Whether or not the display is greyscale
    pub fn is_grayscale(&self) -> bool {
        self.format == PixelFormat::U8
    }

    fn draw_pixel_at_byte(&mut self, byte_index: usize, pixel: RgbPixel) {
        match self.format {
            PixelFormat::U8 => self.back_buffer[byte_index] = pixel.average(),

            PixelFormat::Rgb => {
                self.back_buffer[byte_index] = pixel.red;
                self.back_buffer[byte_index + 1] = pixel.green;
                self.back_buffer[byte_index + 2] = pixel.blue;
            }

            PixelFormat::Bgr => {
                self.back_buffer[byte_index] = pixel.blue;
                self.back_buffer[byte_index + 1] = pixel.green;
                self.back_buffer[byte_index + 2] = pixel.red;
            },

            _ => unimplemented!("Unsupported pixel format"),
        }
    }

    /// Draws a pixel at a given position to the back-buffer
    pub fn draw_pixel(&mut self, x: usize, y: usize, pixel: RgbPixel) {
        let px_index = y * self.stride + x;
        let b_index = px_index * self.bytes_per_pixel;
        self.draw_pixel_at_byte(b_index, pixel);
    }

    /// Clears the back-buffer to black
    pub fn clear(&mut self) {
        let black = RgbPixel { red: 0, green: 0, blue: 0 };
        self.clear_with_color(black);
    }

    /// Clears the back-buffer to a given color
    pub fn clear_with_color(&mut self, color: RgbPixel) {
        for pixel in 0..(self.stride * self.height) {
            self.draw_pixel_at_byte(pixel * self.bytes_per_pixel, color);
        }
    }

    /// Flushes the back-buffer into the actual framebuffer
    pub fn flush(&mut self) { 
        self.buffer.copy_from_slice(&self.back_buffer);
    }
}

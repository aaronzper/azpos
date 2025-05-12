use core::fmt;
use simple_pcf::Pcf;
use crate::devices::fb::{Framebuffer, RgbPixel};

const FONT: &'static [u8] = include_bytes!("font.psf");
const TAB_WIDTH: usize = 8;
const MAX_WIDTH: usize = 8; // hardcode this width so we dont get crazy space
                            // between chars (we know the font so doesnt matter
                            // anyway)

pub struct FbTerminal {
    font: Pcf<'static>,
    fb: Framebuffer,

    row: usize,
    col: usize,

    fg: RgbPixel,
    bg: RgbPixel,
}

impl FbTerminal {
    pub fn new(mut fb: Framebuffer) -> FbTerminal {
        let font = Pcf::parse(FONT).unwrap();

        fb.clear();

        FbTerminal { 
            font, fb, 
            row: 0, col: 0,
            fg: RgbPixel { red: 0xFF, green: 0xFF, blue: 0xFF },
            bg: RgbPixel { red: 0, green: 0, blue: 0}, 
        }
    }

    pub fn width(&self) -> usize {
        self.fb.get_width() / MAX_WIDTH
    }

    pub fn height(&self) -> usize {
        self.fb.get_height() / self.font.glyph_height
    }

    pub fn write_char(&mut self, c: char) {
        if self.row >= self.height() {
            self.fb.clear();
            self.row = 0;
            self.col = 0;
        }

        match c {
            '\n' => {
                self.row += 1;
                self.col = 0;
            },

            '\r' => { self.col = 0; },

            '\t' => {
                let mut offset = TAB_WIDTH - (self.col % TAB_WIDTH);
                if offset == 0 { offset = TAB_WIDTH; }

                self.col += offset;
            },

            c => {
                match self.font.get_glyph_pixels(c as usize) {
                    Some(pixels) => {
                        for (i, p) in pixels.enumerate() {
                            let x = 
                                (i % self.font.glyph_width) 
                                + (self.col * MAX_WIDTH);
                            let y = 
                                (i / self.font.glyph_width)
                                + (self.row * self.font.glyph_height);

                            let color = if p {
                                self.fg
                            } else {
                                self.bg
                            };

                            self.fb.draw_pixel(x, y, color);
                        }
                    },

                    None => panic!("Invalid char"),
                }

                self.col += 1;
            }
        }

        if self.col >= self.width() {
            self.col = 0;
            self.row += 1;
        }
    }
}

impl fmt::Write for FbTerminal {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.write_char(c);
        }
        Ok(())
    }
}

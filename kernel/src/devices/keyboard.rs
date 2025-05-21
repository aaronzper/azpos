use spin::Mutex;
use x86_64::instructions::port::Port;

const KEYBOARD_PORT: u16 = 0x60;

pub static KEYBOARD: Mutex<Keyboard> = Mutex::new(Keyboard::new());

/// A PS/2 keyboard
pub struct Keyboard {
    port: Port<u8>,
    caps: bool
}

impl Keyboard {
    pub const fn new() -> Keyboard {
        Keyboard {
            port: Port::new(KEYBOARD_PORT),
            caps: false
        }
    }

    pub fn read_scancode(&mut self) -> u8 {
        let scancode = unsafe {
            self.port.read()
        };

        // Handle metadata side-effects (e.g. tracking caps)
        match scancode {
            // Press CapsLock, Press Left/Right Shift, Release Left/Right shift
            0x3A | 0x2A | 0x36 | 0xAA | 0xB6 => {
                self.caps = !self.caps;
            }
            
            _ => ()
        };

        scancode
    }

    pub fn read_char(&mut self) -> Option<char> {
        let res = match self.read_scancode() {
            0x02 => Some('1'),
            0x03 => Some('2'),
            0x04 => Some('3'),
            0x05 => Some('4'),
            0x06 => Some('5'),
            0x07 => Some('6'),
            0x08 => Some('7'),
            0x09 => Some('8'),
            0x0A => Some('9'),
            0x0B => Some('0'),
            0x0C => Some('-'),
            0x0D => Some('='),
            0x0F => Some('\t'),
            0x10 => Some('q'),
            0x11 => Some('w'),
            0x12 => Some('e'),
            0x13 => Some('r'),
            0x14 => Some('t'),
            0x15 => Some('y'),
            0x16 => Some('u'),
            0x17 => Some('i'),
            0x18 => Some('o'),
            0x19 => Some('p'),
            0x1A => Some('['),
            0x1B => Some(']'),
            0x1C => Some('\n'),
            0x1E => Some('a'),
            0x1F => Some('s'),
            0x20 => Some('d'),
            0x21 => Some('f'),
            0x22 => Some('g'),
            0x23 => Some('h'),
            0x24 => Some('j'),
            0x25 => Some('k'),
            0x26 => Some('l'),
            0x27 => Some(';'),
            0x28 => Some('\''),
            0x29 => Some('`'),
            0x2B => Some('\\'),
            0x2C => Some('z'),
            0x2D => Some('x'),
            0x2E => Some('c'),
            0x2F => Some('v'),
            0x30 => Some('b'),
            0x31 => Some('n'),
            0x32 => Some('m'),
            0x33 => Some(','),
            0x34 => Some('.'),
            0x35 => Some('/'),
            0x39 => Some(' '),
            _ => None
        };

        match res {
            Some(c) if self.caps => Some(c.to_ascii_uppercase()),
            _ => res
        }
    }
}

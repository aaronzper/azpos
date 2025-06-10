use scancode::Scancode;
use crate::scheduling::threads::sync::{Buffer, KIntMutex};
use x86_64::instructions::port::Port;

mod scancode;

const KEYBOARD_PORT: u16 = 0x60;

pub static KEYBOARD: KIntMutex<Keyboard> = KIntMutex::new(Keyboard::new());
pub static SCANCODES: Buffer<Scancode, 64> = Buffer::new();

/// A PS/2 keyboard
pub struct Keyboard {
    port: Port<u8>,
}

impl Keyboard {
    pub const fn new() -> Keyboard {
        Keyboard {
            port: Port::new(KEYBOARD_PORT),
        }
    }

    pub fn read_scancode(&mut self) -> Option<Scancode> {
        let raw_code = unsafe {
            self.port.read()
        };

        match Scancode::try_from(raw_code) {
            Ok(code) => Some(code),
            Err(_) => None,
        }
    }
}

pub fn keyboard_listener() {
    loop {
        let scode = SCANCODES.pop();
        println!("{:?}", scode);
    }
}

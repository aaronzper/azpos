use alloc::vec::Vec;
use scancode::Scancode;
use crate::scheduling::{kthread_yield, threads::mutex::KMutex};
use x86_64::instructions::port::Port;

mod scancode;

const KEYBOARD_PORT: u16 = 0x60;

pub static KEYBOARD: KMutex<Keyboard> = KMutex::new(Keyboard::new());
pub static SCANCODES: KMutex<Vec<Scancode>> = KMutex::new(Vec::new());

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
        let mut buf = Vec::new();

        crate::interrupts::without_interrupts(|| {
            let mut scodes = SCANCODES.lock();
            buf.extend_from_slice(scodes.as_slice());
            scodes.clear();
        });

        if buf.is_empty() {
            kthread_yield();
            continue;
        }

        for scode in buf {
            println!("{:?}", scode);
        }
    }
}

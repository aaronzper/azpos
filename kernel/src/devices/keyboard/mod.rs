use alloc::{boxed::Box, string::String};
use libsci::{devices::{DeviceInfo, DriverInfo}, resources::Resource};
use scancode::Scancode;
use crate::scheduling::threads::sync::{Buffer, KIntMutex};
use x86_64::instructions::port::Port;
use super::DeviceDriver;
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


/// The `DeviceDriver` for PS/2 exposed to processes
pub struct KeyboardDriver;

impl DeviceDriver for KeyboardDriver {
    fn driver_info(&self) -> DriverInfo {
        let kb = DeviceInfo {
            device_name: String::from("keyboard"),
            device_type: libsci::devices::DeviceType::Keyboard,
        };

        DriverInfo { 
            driver_name: String::from("ps2_keyboard"),
            devices: Box::from([kb]),
        }
    }

    fn open_device(&mut self, device_name: &str) -> Option<Box<dyn Resource>> {
        todo!()
    }
}

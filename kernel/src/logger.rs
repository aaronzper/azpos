use core::fmt::Write;
use spin::Mutex;
use uart_16550::SerialPort;

use crate::devices::fb::FbTerminal;

static LOGGER: Mutex<Option<FbTerminal>> = Mutex::new(None);
static SERIAL: Mutex<Option<SerialPort>> = Mutex::new(None);

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::logger::_log(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _log(args: core::fmt::Arguments) {
    let mut serial_lock = SERIAL.lock();
    match serial_lock.as_mut() {
        Some(s) => {
            s.write_fmt(args).unwrap();
        },
        None => {
            let mut s = unsafe {
                SerialPort::new(0x3F8)
            };
            s.init();
            s.write_fmt(args).unwrap();
            *serial_lock = Some(s);
        }
    }

    let mut logger_lock = LOGGER.lock();
    match logger_lock.as_mut() {
        Some(l) => l.write_fmt(args).unwrap(),
        None => ()
    }
}

// Temporary until I get to ANSII escape codes lol
pub fn set_fg_color(color: crate::devices::fb::RgbPixel) {
    let mut lock = LOGGER.lock();
    lock.as_mut().unwrap().set_fg(color);
}

pub fn set_logger(logger: FbTerminal) {
    let mut lock = LOGGER.lock();
    *lock = Some(logger);
}

pub fn logger_initialized() -> bool {
    LOGGER.lock().is_some()
}

use core::fmt::Write;
use spin::Mutex;
use crate::devices::{fb::FbTerminal, serial::SerialPort};

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
            s
        },
        None => {
            let mut s = SerialPort::new();
            *serial_lock = Some(s);
            serial_lock.as_mut().unwrap()
        }
    }.write_fmt(args).unwrap();

    let mut logger_lock = LOGGER.lock();
    match logger_lock.as_mut() {
        Some(l) => {
            l.write_fmt(args).unwrap();
            l.flush();
        },
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

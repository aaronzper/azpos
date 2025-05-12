use core::fmt::Write;
use spin::Mutex;

use crate::devices::fb::FbTerminal;

static LOGGER: Mutex<Option<FbTerminal>> = Mutex::new(None);

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
    let mut lock = LOGGER.lock();
    lock.as_mut().unwrap().write_fmt(args).unwrap();
}

pub fn set_logger(logger: FbTerminal) {
    let mut lock = LOGGER.lock();
    *lock = Some(logger);
}

pub fn logger_initialized() -> bool {
    LOGGER.lock().is_some()
}

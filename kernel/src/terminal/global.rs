use super::Terminal;
use core::fmt::Write;
use spin::Mutex;

static TERMINAL: Mutex<Option<Terminal>> = Mutex::new(None);

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::terminal::global::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    let mut lock = TERMINAL.lock();
    lock.as_mut().unwrap().write_fmt(args).unwrap();
}

pub fn set_global_terminal(t: Terminal) {
    let mut lock = TERMINAL.lock();
    *lock = Some(t);
}

pub fn global_terminal_initialized() -> bool {
    TERMINAL.lock().is_some()
}

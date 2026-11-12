use core::fmt::Write;
use crate::{devices::{fb::FbTerminal, serial::SerialPort}, scheduling::threads::sync::KMutex};

static LOGGER: KMutex<Option<FbTerminal>> = KMutex::new(None);
static SERIAL: KMutex<SerialPort> = KMutex::new(SerialPort::new());

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
    serial_lock.write_fmt(args).unwrap();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktest::*;
    use crate::processes::{tests::spawn_test_program, PROCESSES};
    use crate::scheduling::thread_yield;

    /// The number of yields in each step of the case
    const YIELDS: usize = 20;

    ktest! {
        /// A process can write to the log while a kernel thread holds the serial lock.
        /// The process waits for the lock, and the kernel continues (GitHub #29).
        /// Today the syscall holds `PROCESSES` with interrupts off, then blocks on `SERIAL`.
        /// The kernel then panics with "Tried to yield with interrupts disabled".
        #[known_bug(29, crash)]
        fn a_process_waits_for_the_serial_lock() {
            let pid = spawn_test_program("logloop").or_fail("spawn logloop")?;

            // The process has its logger when it has the resource 0
            let has_logger = wait_until(|| {
                PROCESSES.lock().get_entry(pid).is_some_and(|p| p.resources.get_entry(0).is_some())
            });
            check!(has_logger, "the process did not get a logger");

            // The case must not print while it holds the lock
            let lock = SERIAL.lock();
            for _ in 0..YIELDS {
                thread_yield();
            }
            drop(lock);

            for _ in 0..YIELDS {
                thread_yield();
            }
        }
    }
}

use core::{fmt::{self, Write}, panic::PanicInfo};

use crate::syscalls::log_raw;

unsafe extern "C" { 
    fn main();
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    unsafe { main(); }

    loop {}
}

/// The length of the longest panic message that the process logs
const PANIC_TEXT_LEN: usize = 512;

/// A fixed text buffer. The panic handler must not allocate.
struct PanicText {
    bytes: [u8; PANIC_TEXT_LEN],
    len: usize,
}

impl Write for PanicText {
    /// Adds the text on one line. A line break becomes a space. Text that
    /// does not fit is dropped.
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            let c = if c == '\n' || c == '\r' { ' ' } else { c };

            let end = self.len + c.len_utf8();
            if end > PANIC_TEXT_LEN {
                break;
            }
            c.encode_utf8(&mut self.bytes[self.len..end]);
            self.len = end;
        }
        Ok(())
    }
}

/// Logs the panic message on the kernel log, then spins.
///
/// The test harness looks for the text `!!! USER PANIC !!!` on the serial
/// port (`azpos::session`).
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut text = PanicText { bytes: [0; PANIC_TEXT_LEN], len: 0 };
    let _ = write!(text, "!!! USER PANIC !!! {info}");
    log_raw(&text.bytes[..text.len]);

    loop {}
}

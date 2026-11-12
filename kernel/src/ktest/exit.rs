use core::{fmt::{self, Write}, panic::PanicInfo};

use libtestkit::protocol::PREFIX;
use x86_64::instructions::port::Port;

use crate::interrupts::disable_interrupts;

/// I/O port of the QEMU device `isa-debug-exit`. The host adds the device
/// with `-device isa-debug-exit,iobase=0xf4,iosize=0x04`.
const PORT_DEBUG_EXIT: u16 = 0xf4;

const PORT_SERIAL_DATA: u16 = 0x3F8;
const PORT_SERIAL_LINE_STATUS: u16 = 0x3FD;
/// Line status bit: the transmit register is empty
const SERIAL_TRANSMIT_EMPTY: u8 = 0x20;

/// A value for the `isa-debug-exit` device. QEMU exits with the status
/// `(value << 1) | 1`.
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum ExitCode {
    /// All suites of this boot are complete. QEMU exit status 33.
    Success = 0x10,
    /// The kernel panicked. QEMU exit status 35.
    Panic = 0x11,
}

/// Stops QEMU with the given code
pub fn exit_qemu(code: ExitCode) -> ! {
    unsafe { Port::<u32>::new(PORT_DEBUG_EXIT).write(code as u32) };

    // QEMU stops on the port write. This loop runs only without the device.
    loop {
        x86_64::instructions::hlt();
    }
}

/// Writes to the serial port with no lock. Only the panic handler uses it,
/// because the panic can occur while a thread holds the logger lock.
struct RawSerial;

impl RawSerial {
    fn write_byte(&mut self, byte: u8) {
        unsafe {
            let mut status = Port::<u8>::new(PORT_SERIAL_LINE_STATUS);
            while status.read() & SERIAL_TRANSMIT_EMPTY == 0 {}
            Port::<u8>::new(PORT_SERIAL_DATA).write(byte);
        }
    }
}

impl Write for RawSerial {
    /// Writes the text on one line. A line break becomes a space.
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            match byte {
                b'\n' | b'\r' => self.write_byte(b' '),
                _ => self.write_byte(byte),
            }
        }
        Ok(())
    }
}

/// The panic handler of a test build. It prints one `panic` record and
/// stops QEMU. It does not allocate and takes no lock.
pub fn on_panic(info: &PanicInfo) -> ! {
    disable_interrupts();

    let mut serial = RawSerial;
    // The panic can occur in the middle of a line
    serial.write_byte(b'\n');
    let _ = write!(serial, "{PREFIX}panic ");
    if let Some(location) = info.location() {
        let _ = write!(serial, "{}:{}:{}: ", location.file(), location.line(), location.column());
    }
    let _ = write!(serial, "{}", info.message());
    serial.write_byte(b'\n');

    exit_qemu(ExitCode::Panic)
}

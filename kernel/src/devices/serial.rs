const SERIAL_PORT: u16 = 0x3F8;

/// A wrapper around a Serial Port device that can be written to
pub struct SerialPort {
    inner: uart_16550::SerialPort,
}

impl SerialPort {
    /// Creates a new `SerialPort`
    pub const fn new() -> SerialPort {
        SerialPort {
            inner: unsafe {
                uart_16550::SerialPort::new(SERIAL_PORT)
            }
        }
    }
}

impl core::fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.inner.write_str(s)
    }
}

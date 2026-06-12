const SERIAL_PORT: u16 = 0x3F8;

/// A wrapper around a Serial Port device that can be written to
pub struct SerialPort {
    inner: uart_16550::SerialPort,
    initialized: bool,
}

impl SerialPort {
    /// Creates a new `SerialPort`
    pub const fn new() -> SerialPort {
        SerialPort {
            inner: unsafe {
                uart_16550::SerialPort::new(SERIAL_PORT)
            },
            initialized: false,
        }
    }
}

impl core::fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if !self.initialized {
            unsafe { self.inner.init() };
            self.initialized = true;
        }
        self.inner.write_str(s)
    }
}

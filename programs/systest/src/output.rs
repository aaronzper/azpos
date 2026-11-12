use libsystem::{libsci::resources::ResourceID, syscalls::{sys_get_logger, sys_write}};

/// Writes lines to the kernel log. The kernel prints each write as one line
/// on the serial port, with `[PID n] ` before it.
pub struct Output {
    logger: ResourceID,
}

impl Output {
    pub fn new() -> Self {
        Self { logger: sys_get_logger() }
    }

    /// Writes one line. A write error is ignored, because the program has no
    /// other place to report it.
    pub fn line(&mut self, line: &str) {
        let _ = sys_write(self.logger, line.as_bytes());
    }
}

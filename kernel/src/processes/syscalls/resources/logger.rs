use core::cmp::min;
use libsci::resources::{Resource, ResourceError};
use crate::scheduling::SCHEDULER;

/// Allows processes to write to the kernel log
pub struct LoggerResource;

impl LoggerResource {
    pub fn new() -> Self {
        let pid = SCHEDULER.lock().current_proc().unwrap();
        println!("PID {pid} attatched to logger");

        Self
    }
}

impl Resource for LoggerResource {
    fn write(&mut self, buffer: &[u8]) -> libsci::resources::ResourceResult {
        let pid = SCHEDULER.lock().current_proc().unwrap();
        let msg = match str::from_utf8(buffer) {
            Ok(s) => s,
            Err(_) => {
                return Err(ResourceError::InvalidInput);
            },
        };
        println!("[PID {pid}] {msg}");

        Ok(msg.len() as i64)
    }

    fn seek(&mut self, _: usize) -> libsci::resources::ResourceResult {
        Err(ResourceError::Unsupported)
    }

    fn read(&mut self, _: &mut [u8]) -> libsci::resources::ResourceResult {
        Err(ResourceError::Unsupported)
    }
}

impl Drop for LoggerResource {
    fn drop(&mut self) {
        let pid = SCHEDULER.lock().current_proc().unwrap();
        println!("PID {pid} detatched from logger");
    }
}

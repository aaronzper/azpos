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
    fn write(&mut self, buffer: &[u8]) -> libsci::resources::ResourceResult<u64> {
        let pid = SCHEDULER.lock().current_proc().unwrap();
        let msg = match str::from_utf8(buffer) {
            Ok(s) => s,
            Err(e) => {
                return Err(ResourceError::InvalidInput);
            },
        };
        println!("[PID {pid}] {msg}");

        Ok(msg.len() as u64)
    }

    fn seek(&mut self, _: usize) -> libsci::resources::ResourceResult<()> {
        Err(ResourceError::Unsupported)
    }

    // Temporary test version, will be unsupported long-term
    fn read(&mut self, buf: &mut [u8]) -> libsci::resources::ResourceResult<u64> {
        let src = "Hello world from a syscall".as_bytes();
        
        let len = min(src.len(), buf.len());
        buf[0..len].clone_from_slice(&src[0..len]);

        Ok(len as u64)
    }
}

impl Drop for LoggerResource {
    fn drop(&mut self) {
        let pid = SCHEDULER.lock().current_proc().unwrap();
        println!("PID {pid} detatched from logger");
    }
}

use libsci::resources::{rax_to_result, Resource, ResourceID, ResourceResult};

use super::{sys_close, sys_read, sys_seek, sys_write};

#[derive(Debug)]
/// An azpOS resource. This struct is a wrapper around a resource ID and provides
/// the resource-related syscalls, calling them with it.
pub struct SystemResource {
    rid: ResourceID,
}

impl From<ResourceID> for SystemResource {
    fn from(rid: ResourceID) -> Self {
        Self { rid }
    }
}

impl Resource for SystemResource {
    fn read(&mut self, buffer: &mut [u8]) -> ResourceResult {
        sys_read(self.rid, buffer)
    }

    fn write(&mut self, buffer: &[u8]) -> ResourceResult {
        sys_write(self.rid, buffer)
    }

    fn seek(&mut self, offset: usize) -> ResourceResult {
        sys_seek(self.rid, offset)
    }
}

impl Drop for SystemResource {
    fn drop(&mut self) {
        sys_close(self.rid).unwrap();
    }
}

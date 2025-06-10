use libsci::resources::{parse_resource_result, Resource, ResourceID, ResourceResult};

use super::{sys_close, sys_read, sys_seek, sys_write};

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
    fn read(&mut self, buffer: &mut [u8]) -> ResourceResult<u64> {
        let res = sys_read(self.rid, buffer);
        parse_resource_result(res)
    }

    fn write(&mut self, buffer: &[u8]) -> ResourceResult<u64> {
        let res = sys_write(self.rid, buffer);
        parse_resource_result(res)
    }

    fn seek(&mut self, offset: usize) -> ResourceResult<()> {
        let res = sys_seek(self.rid, offset);
        match parse_resource_result::<u64>(res) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

impl Drop for SystemResource {
    fn drop(&mut self) {
        sys_close(self.rid);
    }
}

use libsystem::{
    libsci::resources::{ResourceError, ResourceID},
    syscalls::{sys_read, sys_seek, sys_write},
};
use libtestkit::{cases, check_matches};

/// A resource ID that no process has
const NO_RESOURCE: ResourceID = 0xFFFF_FF00;

cases! {
    fn read_of_an_unknown_resource_gives_not_found() {
        let mut buf = [0u8; 8];
        check_matches!(sys_read(NO_RESOURCE, &mut buf), Err(ResourceError::ResourceNotFound));
    }

    fn write_of_an_unknown_resource_gives_not_found() {
        check_matches!(sys_write(NO_RESOURCE, b"x"), Err(ResourceError::ResourceNotFound));
    }

    fn seek_of_an_unknown_resource_gives_not_found() {
        check_matches!(sys_seek(NO_RESOURCE, 0), Err(ResourceError::ResourceNotFound));
    }
}

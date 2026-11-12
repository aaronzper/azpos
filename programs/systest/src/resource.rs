use libsystem::{
    libsci::resources::{ResourceError, ResourceID},
    alloc::vec::Vec,
    syscalls::{sys_close, sys_get_logger, sys_list_devices, sys_read, sys_seek, sys_write},
};
use libtestkit::{cases, check_eq, check_matches};

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

    /// Today `sys_close` ignores the result of `remove_entry` and returns 0.
    #[known_bug(34)]
    fn close_of_an_unknown_resource_gives_not_found() {
        check_matches!(sys_close(NO_RESOURCE), Err(ResourceError::ResourceNotFound));
    }

    /// Today the second close returns 0.
    #[known_bug(34)]
    fn second_close_of_a_resource_gives_not_found() {
        let logger = sys_get_logger();
        check_matches!(sys_close(logger), Ok(_));
        check_matches!(sys_close(logger), Err(ResourceError::ResourceNotFound));
    }

    fn one_hundred_loggers_have_different_ids() {
        let mut ids: Vec<ResourceID> = (0..100).map(|_| sys_get_logger()).collect();
        let mut closed = 0;
        for &id in &ids {
            if sys_close(id).is_ok() {
                closed += 1;
            }
        }
        check_eq!(closed, 100);

        ids.sort_unstable();
        ids.dedup();
        check_eq!(ids.len(), 100);
    }

    fn close_gives_zero() {
        let logger = sys_get_logger();
        check_matches!(sys_close(logger), Ok(0));
    }

    fn read_and_seek_after_close_give_not_found() {
        let blob = sys_list_devices();
        let mut buf = [0u8; 8];
        check_matches!(sys_close(blob), Ok(_));

        check_matches!(sys_read(blob, &mut buf), Err(ResourceError::ResourceNotFound));
        check_matches!(sys_seek(blob, 0), Err(ResourceError::ResourceNotFound));
    }
}

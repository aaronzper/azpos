use libsystem::{
    libsci::resources::ResourceError,
    syscalls::{sys_close, sys_get_logger, sys_read, sys_seek, sys_write},
};
use libtestkit::{cases, check_matches, check_ne};

cases! {
    fn get_logger_gives_a_new_resource_each_time() {
        let a = sys_get_logger();
        let b = sys_get_logger();
        check_ne!(a, b);

        check_matches!(sys_close(a), Ok(_));
        check_matches!(sys_close(b), Ok(_));
    }

    fn write_returns_the_number_of_bytes() {
        let logger = sys_get_logger();
        check_matches!(sys_write(logger, b"systest: logger write"), Ok(21));
        check_matches!(sys_close(logger), Ok(_));
    }

    fn read_and_seek_are_unsupported() {
        let logger = sys_get_logger();
        let mut buf = [0u8; 8];

        check_matches!(sys_read(logger, &mut buf), Err(ResourceError::Unsupported));
        check_matches!(sys_seek(logger, 0), Err(ResourceError::Unsupported));
        check_matches!(sys_close(logger), Ok(_));
    }

    fn write_after_close_gives_not_found() {
        let logger = sys_get_logger();
        check_matches!(sys_close(logger), Ok(_));
        check_matches!(sys_write(logger, b"x"), Err(ResourceError::ResourceNotFound));
    }

    /// The kernel returns `InvalidInput` (-3) for text that is not UTF-8.
    /// Today `rax_to_result` in `libsci` has no arm for -3 and panics.
    #[known_bug(34, crash)]
    fn write_of_invalid_utf8_gives_invalid_input() {
        let logger = sys_get_logger();
        check_matches!(sys_write(logger, &[0xFF, 0xFE]), Err(ResourceError::InvalidInput));
        check_matches!(sys_close(logger), Ok(_));
    }
}

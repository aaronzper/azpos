use libsystem::{
    libsci::resources::ResourceError,
    syscalls::{sys_close, sys_get_logger, sys_list_devices, sys_read, sys_seek, sys_write},
};
use libtestkit::{cases, check, check_matches, check_ne};

use crate::devices::read_to_end;

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

    fn write_of_zero_bytes_gives_zero() {
        let logger = sys_get_logger();
        check_matches!(sys_write(logger, b""), Ok(0));
        check_matches!(sys_close(logger), Ok(_));
    }

    fn write_of_4096_bytes_gives_4096() {
        let logger = sys_get_logger();
        let text = [b'a'; 4096];
        check_matches!(sys_write(logger, &text), Ok(4096));
        check_matches!(sys_close(logger), Ok(_));
    }

    /// The length counts bytes, not characters.
    /// Today the framebuffer terminal has no glyph for these characters and panics.
    #[known_bug(65, crash)]
    fn write_of_multibyte_text_gives_the_length_in_bytes() {
        let logger = sys_get_logger();
        let text = "systest: \u{e9}\u{20ac}";
        check_matches!(sys_write(logger, text.as_bytes()), Ok(14));
        check_matches!(sys_close(logger), Ok(_));
    }

    fn a_second_logger_works_after_the_close_of_the_first() {
        let first = sys_get_logger();
        let second = sys_get_logger();
        check_matches!(sys_close(first), Ok(_));

        check_matches!(sys_write(second, b"systest: second logger"), Ok(22));
        check_matches!(sys_close(second), Ok(_));
    }

    /// A NUL byte is valid UTF-8, so the write accepts it.
    fn write_of_a_nul_byte_gives_one() {
        let logger = sys_get_logger();
        check_matches!(sys_write(logger, b"\0"), Ok(1));
        check_matches!(sys_close(logger), Ok(_));
    }

    /// Text that ends in the middle of a character is not UTF-8.
    /// Today `rax_to_result` has no arm for -3 and panics.
    #[known_bug(34, crash)]
    fn write_of_a_cut_character_gives_invalid_input() {
        let logger = sys_get_logger();
        let text = "\u{20ac}".as_bytes();
        check_matches!(sys_write(logger, &text[..2]), Err(ResourceError::InvalidInput));
        check_matches!(sys_close(logger), Ok(_));
    }

    /// The close of a logger does not change a resource of another type.
    fn close_of_a_logger_keeps_a_blob() {
        let blob = sys_list_devices();
        let logger = sys_get_logger();
        check_matches!(sys_close(logger), Ok(_));

        let data = read_to_end(blob)?;
        check!(!data.is_empty());
        check_matches!(sys_close(blob), Ok(_));
    }
}

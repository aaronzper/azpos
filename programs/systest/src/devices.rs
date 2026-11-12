use libsystem::{
    alloc::{boxed::Box, vec, vec::Vec},
    libsci::{
        devices::{DeviceType, DriverInfo},
        postcard::from_bytes,
        resources::{ResourceError, ResourceID},
    },
    syscalls::{sys_close, sys_list_devices, sys_read, sys_seek, sys_write},
};
use libtestkit::{cases, check, check_eq, check_matches, check_ne, OrFail, TestResult, Failure};

/// Reads a resource from its current position to its end, in small pieces
pub fn read_to_end(rid: ResourceID) -> Result<Vec<u8>, Failure> {
    let mut data = Vec::new();
    loop {
        let mut piece = [0u8; 16];
        let len = sys_read(rid, &mut piece).or_fail("read")? as usize;
        if len == 0 {
            return Ok(data);
        }
        data.extend_from_slice(&piece[..len]);
    }
}

fn has_keyboard(drivers: &[DriverInfo]) -> TestResult {
    let driver = drivers.iter()
        .find(|driver| driver.driver_name == "ps2_keyboard")
        .or_fail("driver ps2_keyboard")?;
    let device = driver.devices.iter()
        .find(|device| device.device_name == "keyboard")
        .or_fail("device keyboard")?;
    check_matches!(device.device_type, DeviceType::Keyboard);
    Ok(())
}

cases! {
    fn list_devices_gives_the_keyboard_driver() {
        let blob = sys_list_devices();
        let data = read_to_end(blob)?;
        check_matches!(sys_close(blob), Ok(_));

        let drivers: Box<[DriverInfo]> = from_bytes(&data).or_fail("postcard")?;
        has_keyboard(&drivers)?;
    }

    fn the_blob_gives_the_same_bytes_after_seek_to_zero() {
        let blob = sys_list_devices();
        let first = read_to_end(blob)?;
        check!(!first.is_empty());

        check_matches!(sys_seek(blob, 0), Ok(_));
        let second = read_to_end(blob)?;
        check_eq!(first, second);

        check_matches!(sys_close(blob), Ok(_));
    }

    fn the_blob_is_read_only() {
        let blob = sys_list_devices();
        check_matches!(sys_write(blob, b"x"), Err(ResourceError::Unsupported));
        check_matches!(sys_close(blob), Ok(_));
    }

    /// A read after a seek past the end returns 0 bytes, or the seek returns
    /// an error. Today the read panics the kernel.
    #[known_bug(44, crash)]
    fn read_after_seek_past_the_end_does_not_panic_the_kernel() {
        let blob = sys_list_devices();
        let mut buf = [0u8; 4];

        if sys_seek(blob, 1 << 20).is_ok() {
            check_matches!(sys_read(blob, &mut buf), Ok(0));
        }
        check_matches!(sys_close(blob), Ok(_));
    }

    fn read_into_an_empty_buffer_gives_zero_and_keeps_the_position() {
        let blob = sys_list_devices();
        let all = read_to_end(blob)?;
        check_matches!(sys_seek(blob, 0), Ok(_));

        check_matches!(sys_read(blob, &mut []), Ok(0));
        let after = read_to_end(blob)?;
        check_eq!(all, after);
        check_matches!(sys_close(blob), Ok(_));
    }

    fn reads_of_each_piece_size_give_the_same_bytes() {
        let blob = sys_list_devices();
        let expected = read_to_end(blob)?;

        for size in [1usize, 7, 64, 4096] {
            check_matches!(sys_seek(blob, 0), Ok(_));
            let mut piece = vec![0u8; size];
            let mut data = Vec::new();
            loop {
                let len = sys_read(blob, &mut piece).or_fail("read")? as usize;
                check!(len <= size, "read of {} bytes into {} bytes", len, size);
                if len == 0 {
                    break;
                }
                data.extend_from_slice(&piece[..len]);
            }
            check!(data == expected, "piece size {} gave other bytes", size);
        }
        check_matches!(sys_close(blob), Ok(_));
    }

    fn two_blobs_have_separate_positions() {
        let a = sys_list_devices();
        let b = sys_list_devices();
        check_ne!(a, b);

        let from_a = read_to_end(a)?;
        let from_b = read_to_end(b)?;
        check!(!from_b.is_empty());
        check_eq!(from_a, from_b);

        check_matches!(sys_close(a), Ok(_));
        check_matches!(sys_close(b), Ok(_));
    }

    fn seek_to_the_exact_end_then_read_gives_zero() {
        let blob = sys_list_devices();
        let len = read_to_end(blob)?.len();
        let mut buf = [0u8; 8];

        check_matches!(sys_seek(blob, len), Ok(_));
        check_matches!(sys_read(blob, &mut buf), Ok(0));
        check_matches!(sys_close(blob), Ok(_));
    }

    /// The design of `Seek` gives 0 on success.
    fn seek_gives_zero() {
        let blob = sys_list_devices();
        check_matches!(sys_seek(blob, 1), Ok(0));
        check_matches!(sys_close(blob), Ok(_));
    }

    fn seek_to_the_middle_gives_the_tail() {
        let blob = sys_list_devices();
        let all = read_to_end(blob)?;
        let middle = all.len() / 2;

        check_matches!(sys_seek(blob, middle), Ok(_));
        let tail = read_to_end(blob)?;
        check_eq!(&tail[..], &all[middle..]);
        check_matches!(sys_close(blob), Ok(_));
    }

    /// The close of one blob does not change another blob.
    fn close_of_one_blob_keeps_the_other() {
        let a = sys_list_devices();
        let b = sys_list_devices();
        check_matches!(sys_close(a), Ok(_));

        let data = read_to_end(b)?;
        check!(!data.is_empty());
        check_matches!(sys_close(b), Ok(_));
    }
}

use libsystem::{
    alloc::{boxed::Box, vec::Vec},
    libsci::{
        devices::{DeviceType, DriverInfo},
        postcard::from_bytes,
        resources::{ResourceError, ResourceID},
    },
    syscalls::{sys_close, sys_list_devices, sys_read, sys_seek, sys_write},
};
use libtestkit::{cases, check, check_eq, check_matches, OrFail, TestResult, Failure};

/// Reads a resource from its current position to its end, in small pieces
fn read_to_end(rid: ResourceID) -> Result<Vec<u8>, Failure> {
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
}

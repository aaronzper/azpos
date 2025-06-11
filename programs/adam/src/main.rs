#![no_std]
#![no_main]

use libsystem::alloc::boxed::Box;
use libsystem::alloc::format;
use libsystem::alloc::vec::Vec;
use libsystem::libsci::devices::DriverInfo;
use libsystem::libsci::postcard::from_bytes;
use libsystem::syscalls::{sys_get_logger, sys_list_devices, SystemResource};
use libsystem::libsci::resources::Resource;

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    let mut logger: SystemResource = sys_get_logger().into();
    logger.write("Hello world from a resource! I love syscalls!".as_bytes()).unwrap();

    let mut drivers_blob: SystemResource = sys_list_devices().into();
    let mut buf = Vec::new();
    loop {
        let mut this_buf = [0u8; 64];
        let num_read = drivers_blob.read(&mut this_buf).unwrap() as usize;
        if num_read == 0 { break; }
        buf.extend_from_slice(&this_buf[..num_read]);
    }

    for byte in &buf {
        logger.write(format!("{byte:#X}").as_bytes()).unwrap();
    }

    let drivers: Box<[DriverInfo]> = from_bytes(&buf).unwrap();
    logger.write(format!("{drivers:#?}").as_bytes()).unwrap();
}

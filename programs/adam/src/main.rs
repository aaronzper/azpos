#![no_std]
#![no_main]

use libsystem::alloc::format;
use libsystem::syscalls::{sys_get_logger, SystemResource};
use libsystem::libsci::resources::Resource;

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    let mut logger: SystemResource = sys_get_logger().into();
    logger.write("Hello world from a resource! I love syscalls!".as_bytes()).unwrap();

    let mut buf = [0u8; 100];
    let len = logger.read(&mut buf).unwrap() as usize;
    logger.write(format!("Over-size: '{}'", str::from_utf8(&buf[0..len]).unwrap()).as_bytes()).unwrap();

    let mut buf = [0u8; 5];
    let len = logger.read(&mut buf).unwrap() as usize;
    logger.write(format!("Under-size: '{}'", str::from_utf8(&buf[0..len]).unwrap()).as_bytes()).unwrap();
}

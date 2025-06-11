#![no_std]
#![no_main]

use libsystem::alloc::format;
use libsystem::syscalls::{sys_get_logger, SystemResource};
use libsystem::libsci::resources::Resource;

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    let mut logger: SystemResource = sys_get_logger().into();
    logger.write("Hello world from a resource! I love syscalls!".as_bytes()).unwrap();

    logger.write(format!("The logger RID is '{:?}'", logger).as_bytes());
}

use x86_64::{instructions::port::{PortGeneric, PortReadOnly, PortWriteOnly}, structures::port::{PortRead, PortWrite}};

/// System device manager
mod manager;
pub use manager::*;
/// Framebuffer driver
pub mod fb;
/// Serial port driver
pub mod serial;
/// Programmable Interrupt Controller driver
pub mod pic;
/// PS/2 keyboard driver
pub mod keyboard;
/// Storage device drivers
pub mod storage;
/// PCI bus driver
pub mod pci;

fn read_port<T: PortRead>(port: u16) -> T {
    let mut p: PortReadOnly<T> = PortGeneric::new(port);
    unsafe { p.read() }
}

fn write_port<T: PortWrite>(port: u16, value: T) {
    let mut p: PortWriteOnly<T> = PortGeneric::new(port);
    unsafe { p.write(value) }
}


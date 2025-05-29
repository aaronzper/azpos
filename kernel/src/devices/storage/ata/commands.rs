use x86_64::structures::port::PortWrite;

#[non_exhaustive]
#[repr(u8)]
pub enum ATACommand {
    IDENTIFY = 0xEC,
}

impl PortWrite for ATACommand {
    unsafe fn write_to_port(port: u16, value: Self) {
        unsafe { u8::write_to_port(port, value as u8); }
    }
}

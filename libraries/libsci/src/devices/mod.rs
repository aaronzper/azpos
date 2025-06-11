use alloc::{boxed::Box, string::String};
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
/// Types of device that user procs can access
pub enum DeviceType {
    Framebuffer,
    Keyboard,
}

#[derive(Debug, Serialize, Deserialize)]
/// Info on a driver, including its child devices
pub struct DriverInfo {
    /// Name of the driver e.g. "ps2_keyboard". Should be unique.
    pub driver_name: String,
    /// The devices on the driver
    pub devices: Box<[DeviceInfo]>,
}

/// Info on a particular device
#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// The name of the device. Should be unique within the driver
    pub device_name: String,
    pub device_type: DeviceType,
}

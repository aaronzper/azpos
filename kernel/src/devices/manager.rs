use alloc::{boxed::Box, collections::btree_set::BTreeSet, vec::Vec};
use libsci::{devices::DriverInfo, resources::Resource};
use crate::scheduling::threads::sync::KMutex;

use super::keyboard::KeyboardDriver;

pub static DEVICE_MANAGER: KMutex<DeviceManager> = KMutex::new(DeviceManager::new());

/// Manages and owns devices that are user-accesible
pub struct DeviceManager {
    drivers: Vec<Box<dyn DeviceDriver>>,
}

impl DeviceManager {
    pub const fn new() -> Self {
        Self {
            drivers: Vec::new(),
        }
    }

    /// Adds a driver to the manager
    pub fn add_driver(&mut self, driver: Box<dyn DeviceDriver>) {
        self.drivers.push(driver);
    }

    /// Gets a device by identifier, which is an alphanumeric ASCII string of
    /// the format:
    ///
    /// `[driver_name]:[device_name]`
    ///
    /// If the identifier isn't found, or if the driver itself chooses to,
    /// this returns `None`
    pub fn get_device(&mut self, device_identifier: &str) -> Option<Box<dyn Resource>> {
        let mut identifier_pieces = device_identifier.split(':');
        let driver_name = identifier_pieces.next()?;
        let device_name = identifier_pieces.next()?;
        if identifier_pieces.next().is_some() {
            // Invalid identifier if it has more than 2 pieces
            return None;
        }

        let driver = self.drivers.iter_mut()
            .find(|drivers| {
                let info = drivers.driver_info();
                info.driver_name == driver_name
            })?;

        driver.open_device(device_name)
    }

    /// Gets the `DriverInfo` on each driver in the manager
    pub fn get_drivers(&self) -> Box<[DriverInfo]> {
        let mut names = BTreeSet::new();
        self.drivers.iter()
            .filter_map(|driver| {
                let info = driver.driver_info();
                if names.contains(&info.driver_name) {
                    None
                } else {
                    names.insert(info.driver_name.clone());
                    Some(info)
                }
            })
            .collect()
    }
}


pub trait DeviceDriver {
    /// Returns information on the driver, including its name and devices.
    ///
    /// NOTE: If the returned driver name has been "claimed" already by another
    /// driver, the drivers info will be thrown out and inaccessible to
    /// processes.
    fn driver_info(&self) -> DriverInfo;

    /// Attempts to open the given device by its name. The driver is free to
    /// return `None` if the device is in use, the name is invalid, etc.
    fn open_device(&mut self, device_name: &str) -> Option<Box<dyn Resource>>;
}

pub fn init_devices() {
    let mut devices = DEVICE_MANAGER.lock();

    devices.add_driver(Box::new(KeyboardDriver));
    // TODO: Many, many more...
}

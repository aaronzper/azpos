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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktest::*;
    use alloc::{rc::Rc, string::{String, ToString}, vec};
    use core::cell::RefCell;
    use libsci::resources::ResourceResult;

    /// A resource that gives `marker` from each read
    struct FakeResource {
        marker: i64,
    }

    impl Resource for FakeResource {
        fn read(&mut self, _buffer: &mut [u8]) -> ResourceResult { Ok(self.marker) }
        fn write(&mut self, _buffer: &[u8]) -> ResourceResult { Ok(0) }
        fn seek(&mut self, _offset: usize) -> ResourceResult { Ok(0) }
    }

    /// A driver that records each name that `open_device` gets
    struct FakeDriver {
        name: &'static str,
        marker: i64,
        opened: Rc<RefCell<Vec<String>>>,
    }

    impl DeviceDriver for FakeDriver {
        fn driver_info(&self) -> DriverInfo {
            DriverInfo { driver_name: self.name.to_string(), devices: Box::new([]) }
        }

        fn open_device(&mut self, device_name: &str) -> Option<Box<dyn Resource>> {
            self.opened.borrow_mut().push(device_name.to_string());
            Some(Box::new(FakeResource { marker: self.marker }))
        }
    }

    /// Builds a manager with one `FakeDriver` for each name.
    /// Driver `i` has the marker `i`.
    fn manager(names: &[&'static str]) -> (DeviceManager, Vec<Rc<RefCell<Vec<String>>>>) {
        let mut manager = DeviceManager::new();
        let mut logs = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let opened = Rc::new(RefCell::new(Vec::new()));
            logs.push(Rc::clone(&opened));
            manager.add_driver(Box::new(FakeDriver { name, marker: i as i64, opened }));
        }
        (manager, logs)
    }

    ktest! {
        fn get_drivers_gives_the_info_of_each_driver() {
            let (manager, _) = manager(&["alpha", "beta"]);
            let names: Vec<String> = manager.get_drivers().iter().map(|d| d.driver_name.clone()).collect();
            check_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);
        }

        fn get_drivers_of_an_empty_manager_is_empty() {
            check_eq!(DeviceManager::new().get_drivers().len(), 0);
        }

        /// The first driver with a name claims it.
        fn get_drivers_gives_only_the_first_driver_with_a_name() {
            let (manager, _) = manager(&["same", "other", "same"]);
            let names: Vec<String> = manager.get_drivers().iter().map(|d| d.driver_name.clone()).collect();
            check_eq!(names, vec!["same".to_string(), "other".to_string()]);
        }

        fn get_device_opens_the_device_of_the_named_driver() {
            let (mut manager, logs) = manager(&["alpha", "drv"]);
            let mut device = manager.get_device("drv:dev").or_fail("get_device")?;
            check_eq!(device.read(&mut []).ok(), Some(1));
            check_eq!(*logs[1].borrow(), vec!["dev".to_string()]);
            check!(logs[0].borrow().is_empty());
        }

        /// `get_device` uses the first driver with a name, as `get_drivers` does.
        fn get_device_uses_the_first_driver_with_a_name() {
            let (mut manager, _) = manager(&["same", "same"]);
            let mut device = manager.get_device("same:x").or_fail("get_device")?;
            check_eq!(device.read(&mut []).ok(), Some(0));
        }

        fn get_device_of_a_bad_identifier_gives_none() {
            let (mut manager, logs) = manager(&["drv"]);
            check!(manager.get_device("drv").is_none());
            check!(manager.get_device("drv:a:b").is_none());
            check!(manager.get_device("other:dev").is_none());
            check!(manager.get_device("").is_none());
            check!(logs[0].borrow().is_empty(), "opened: {:?}", logs[0].borrow());
        }
    }
}

pub fn init_devices() {
    let mut devices = DEVICE_MANAGER.lock();

    devices.add_driver(Box::new(KeyboardDriver));
    // TODO: Many, many more...
}

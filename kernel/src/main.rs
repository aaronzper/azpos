#![no_std]
#![no_main]
#![feature(new_zeroed_alloc)]
#![feature(abi_x86_interrupt)]
#![feature(ascii_char)]
#![allow(dead_code)]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(crate::ktest::run))]
#![cfg_attr(test, reexport_test_harness_main = "ktest_main")]

extern crate alloc;

use alloc::{boxed::Box, string::ToString};
use bootloader_api::{config::Mapping, info::Optional, BootInfo, BootloaderConfig};
use devices::{fb::{FbTerminal, Framebuffer}, init_devices, keyboard::keyboard_listener, pci::{PCIController, PCIDeviceClass}, storage::{ahci::{AHCIController, SATA_PCI_SUBCLASS}, BlockDevice}, DEVICE_MANAGER};
use filesystem::{fat::FATFilesystem, FilePath, FileSystem};
use interrupts::init_interrupts;
use logger::set_logger;
use memory::{init_memory, KERNEL_START_ADDR};
use processes::{spawn_proc, syscalls::init_syscalls, ProcessID};
use scheduling::{thread_yield, threads::Thread, SCHEDULER};

#[macro_use]
/// Global kernel logger
mod logger;
/// General kernel utility types
mod utils;
/// Kernel panic functionality
mod panic;
/// Device drivers
mod devices;
/// Virtual File System and specific file system drivers (e.g. FAT32)
mod filesystem;
/// Memory subsystem (paging, the heap, etc)
mod memory;
/// CPU interrupt subsystem (faults, hardware interrupts)
mod interrupts;
/// Scheduling subsystem (scheduler, threads, processes)
mod scheduling;
/// Userspace processes subsystem (procs, their resources, syscalls, etc)
mod processes;
/// Kernel test harness. Exists only in a test build (`cargo test`).
#[cfg(test)]
mod ktest;

const BOOTCONFIG: BootloaderConfig = {
    let mut conf = BootloaderConfig::new_default();
    conf.mappings.dynamic_range_start = Some(KERNEL_START_ADDR);
    conf.mappings.physical_memory = Some(Mapping::Dynamic);
    conf
};
bootloader_api::entry_point!(kmain, config = &BOOTCONFIG);

/// Initializes the kernel and its subsystems
fn kmain(boot_info: &'static mut BootInfo) -> ! {
    let pmap = boot_info.physical_memory_offset.take().unwrap();
    init_memory(pmap, &boot_info.memory_regions);

    let fb = match &mut boot_info.framebuffer {
        Optional::Some(x) => x,
        Optional::None => panic!("No framebuffer!"),
    };
    let fb_info = fb.info().clone();
    let fb_buf = fb.buffer_mut();
    let fb = Framebuffer::new(fb_buf, fb_info);
    let t = FbTerminal::new(fb);
    set_logger(t);
    println!("Hello world!");

    let mut sched_lock = SCHEDULER.lock();
    sched_lock.add_thread(Thread::new_thread(keyboard_listener, None));
    // A test build runs the kernel tests in place of the init process
    #[cfg(not(test))]
    sched_lock.add_thread(Thread::new_thread(start_init, None));
    #[cfg(test)]
    sched_lock.add_thread(Thread::new_thread(ktest::main_thread, None));

    drop(sched_lock); // Drop so not locked forever

    init_devices();
    init_syscalls();

    // Thread yield should happen right after ints are enabled, so that we
    // dont get pre-empted before the scheduler is running (or in the middle of
    // doing something)
    init_interrupts();
    thread_yield();

    panic!("Scheduler yield returned -- this shouldn't happen!");
}

/// Finds the first SATA drive and mounts its root partition.
///
/// The drive and the file system stay alive until shutdown.
fn mount_root_fs() -> FATFilesystem<'static> {
    mount_root_fs_and_spare().0
}

/// Mounts the root partition, as `mount_root_fs` does.
///
/// Also returns partition 1, the boot partition. The kernel tests write to it.
fn mount_root_fs_and_spare() -> (FATFilesystem<'static>, &'static mut dyn BlockDevice) {
    let pci = PCIController::new();
    let ahci_pci = pci.devices().iter()
        .find(|x| {
            let (c, sc) = x.class();
            c == PCIDeviceClass::MassStorageCtrl && sc == SATA_PCI_SUBCLASS
        })
    .expect("No PCI storage device");
    let ahci = Box::leak(Box::new(AHCIController::new(ahci_pci).unwrap()));

    let device = ahci.devices()[0].take_device().unwrap();
    let partitions = Box::leak(device.partition().unwrap());

    let (first, rest) = partitions.split_at_mut(2);
    let spare = first[1].as_mut();

    (FATFilesystem::mount(rest[0].as_mut()).unwrap(), spare)
}

/// Loads the program at `path` in `fs` and starts it as a process
fn spawn_from_fs(fs: &FATFilesystem, path: &str, name: &str) -> ProcessID {
    let exe = FilePath::new(path.to_string()).unwrap();
    let elf_data = fs.read_all(&exe).expect("Program file not found");

    spawn_proc(name.to_string(), elf_data).expect("Invalid ELF file")
}

/// Mounts the root file system and starts the init process
fn start_init() {
    let fs = mount_root_fs();
    spawn_from_fs(&fs, "/programs/adam.exe", "adam");
}

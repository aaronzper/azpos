#![no_std]
#![no_main]
#![feature(new_zeroed_alloc)]
#![feature(abi_x86_interrupt)]
#![feature(ascii_char)]
#![allow(dead_code)]

extern crate alloc;

use alloc::string::ToString;
use bootloader_api::{config::Mapping, info::Optional, BootInfo, BootloaderConfig};
use devices::{fb::{FbTerminal, Framebuffer}, keyboard::keyboard_listener, pci::{PCIController, PCIDeviceClass}, storage::{ahci::{AHCIController, SATA_PCI_SUBCLASS}, BlockDevice}};
use filesystem::{fat::FATFilesystem, FilePath, FileSystem};
use interrupts::init_interrupts;
use logger::set_logger;
use memory::{init_memory, KERNEL_START_ADDR};
use processes::{spawn_proc, syscalls::init_syscalls};
use scheduling::{thread_yield, threads::Thread, SCHEDULER};

#[macro_use]
/// Global kernel logger
mod logger;
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
    sched_lock.add_thread(Thread::new_thread(|| {
        let pci = PCIController::new();
        let ahci_pci = pci.devices().iter()
            .find(|x| {
                let (c, sc) = x.class();
                c == PCIDeviceClass::MassStorageCtrl && sc == SATA_PCI_SUBCLASS
            })
        .expect("No PCI storage device");
        let mut ahci = AHCIController::new(ahci_pci).unwrap();

        let device = ahci.devices()[0].take_device().unwrap();
        let part = &mut device.partition().unwrap()[2];

        let fs = FATFilesystem::mount(part.as_mut()).unwrap();
        let exe = FilePath::new("/programs/adam.exe".to_string()).unwrap();
        let elf_data = fs.read_all(&exe).unwrap();

        let adam = spawn_proc("adam".to_string(), elf_data.clone()).unwrap();
    }, None));

    drop(sched_lock); // Drop so not locked forever

    init_syscalls();
    init_interrupts();

    thread_yield();

    panic!("Scheduler yield returned -- this shouldn't happen!");
}

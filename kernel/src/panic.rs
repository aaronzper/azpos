use core::panic::PanicInfo;

#[cfg(not(test))]
use crate::{devices::fb::RgbPixel, interrupts::disable_interrupts, logger::{logger_initialized, set_fg_color}, println, scheduling::SCHEDULER};

// A test build reports the panic to the host and stops QEMU.
#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    crate::ktest::on_panic(info)
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    disable_interrupts();

    if logger_initialized() {
        set_fg_color(RgbPixel { red: 0xFF, green: 0, blue: 0 });
    }

    println!("\n!!! KERNEL PANIC !!!");
    if let Some(sched) = SCHEDULER.try_lock() {
        if let Some(tid) = sched.currently_running() {
            print!("From thread {tid} ");

            match sched.get_thread(tid).unwrap().proccess() {
                Some(pid) => println!("(process {pid})"),
                None => println!("(kernel)"),
            }
        }
    }
    println!("{}", info);

    loop {
        x86_64::instructions::hlt();
    }
}

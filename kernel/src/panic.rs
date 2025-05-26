use core::panic::PanicInfo;

use crate::{devices::fb::RgbPixel, interrupts::disable_interrupts, logger::{logger_initialized, set_fg_color}, println, scheduling::SCHEDULER};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    disable_interrupts();

    if logger_initialized() {
        set_fg_color(RgbPixel { red: 0xFF, green: 0, blue: 0 });
    }

    println!("\n!!! KERNEL PANIC !!!");
    if let Some(sched) = SCHEDULER.try_lock() {
        if let Some(tid) = sched.currently_running() {
            println!("From thread {}", tid);
        }
    }
    println!("{}", info);

    loop {
        x86_64::instructions::hlt();
    }
}

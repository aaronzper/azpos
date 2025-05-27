use core::arch::global_asm;

use x86_64::structures::idt::InterruptStackFrame;

use crate::{devices::{keyboard::{KEYBOARD, SCANCODES, SCANCODE_AVAIL}, pic::PICInterrupt}, scheduling::{threads::state::CpuState, SCHEDULER}};

use super::PIC;

global_asm!(include_str!("wrappers.s"));
unsafe extern "C" {
    pub fn timer();
}

pub extern "x86-interrupt" fn breakpoint(stack: InterruptStackFrame) {
    println!("Breakpoint!\n{:#?}", stack);
}

pub extern "x86-interrupt" fn double_fault(stack: InterruptStackFrame, error: u64) -> ! {
    panic!("Double Fault (Error Code {}):\n{:#?}", error, stack);
}

#[unsafe(no_mangle)]
pub extern "C" fn timer_inner(s: &mut CpuState) {
    if let Some(mut guard) = SCHEDULER.try_lock() {
        unsafe { guard.schedule(s); }
    }
    PIC.lock().end_interrupt(PICInterrupt::Timer);
}

pub extern "x86-interrupt" fn keyboard(_: InterruptStackFrame) {
    match KEYBOARD.lock().read_scancode() {
        Some(c) => {
            SCANCODES.lock().push(c);
            SCANCODE_AVAIL.notify_all();
        }
        None => (),
    };
    PIC.lock().end_interrupt(PICInterrupt::Keyboard);
}

use x86_64::structures::idt::InterruptStackFrame;

use crate::devices::{keyboard::KEYBOARD, pic::PICInterrupt};

use super::PIC;

pub extern "x86-interrupt" fn breakpoint(stack: InterruptStackFrame) {
    println!("Breakpoint!\n{:#?}", stack);
}

pub extern "x86-interrupt" fn double_fault(stack: InterruptStackFrame, error: u64) -> ! {
    panic!("Double Fault (Error Code {}):\n{:#?}", error, stack);
}

pub extern "x86-interrupt" fn timer(_: InterruptStackFrame) {
    PIC.lock().end_interrupt(PICInterrupt::Timer);
}

pub extern "x86-interrupt" fn keyboard(_: InterruptStackFrame) {
    match KEYBOARD.lock().read_scancode() {
        Some(c) => println!("{:?}", c),
        None => (),
    };
    PIC.lock().end_interrupt(PICInterrupt::Keyboard);
}

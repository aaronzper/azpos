use lazy_static::lazy_static;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint);
        idt.double_fault.set_handler_fn(double_fault);
        idt
    };
}

pub fn init_interrupts() {
    IDT.load();
}

extern "x86-interrupt" fn breakpoint(stack: InterruptStackFrame) {
    println!("Breakpoint!\n{:#?}", stack);
}

extern "x86-interrupt" fn double_fault(stack: InterruptStackFrame, error: u64) -> ! {
    panic!("Double Fault (Error Code {}):\n{:#?}", error, stack);
}

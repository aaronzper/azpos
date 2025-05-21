use lazy_static::lazy_static;
use x86_64::{registers::segmentation::Segment, structures::{gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector}, idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode}, tss::TaskStateSegment}, VirtAddr};

use crate::memory::PAGE_SIZE;

const INT_STACK_SIZE: usize = PAGE_SIZE as usize * 4;
const INT_STACK_INDEX: usize = 0;

// Needs to be mut so that the stack isn't put into RODATA
static mut INT_STACK: [u8; INT_STACK_SIZE] = [0; INT_STACK_SIZE];

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        unsafe {
            idt.breakpoint.set_handler_fn(breakpoint);
            idt.double_fault.set_handler_fn(double_fault)
                .set_stack_index(INT_STACK_INDEX as u16);
        }

        idt
    };
}

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();

        let stack_beg = VirtAddr::from_ptr(&raw mut INT_STACK);
        let stack_end = stack_beg + INT_STACK_SIZE as u64;
        tss.interrupt_stack_table[INT_STACK_INDEX] = stack_end;

        tss
    };
}

struct GDTSegments {
    code: SegmentSelector,
    data: SegmentSelector,
    tss: SegmentSelector,
}

lazy_static! {
    static ref GDT_SELECTORS: 
        (GlobalDescriptorTable, GDTSegments) = {
        let mut gdt = GlobalDescriptorTable::new();
        
        let code = gdt.append(Descriptor::kernel_code_segment());
        let data = gdt.append(Descriptor::kernel_data_segment());
        let tss = gdt.append(Descriptor::tss_segment(&TSS));

        (gdt, GDTSegments { code, data, tss })
    };
}

/// Initializes interrupts by loading the IDT
pub fn init_interrupts() {
    GDT_SELECTORS.0.load();
    unsafe {
        x86_64::instructions::segmentation::CS::set_reg(GDT_SELECTORS.1.code);
        x86_64::instructions::segmentation::DS::set_reg(GDT_SELECTORS.1.data);
        x86_64::instructions::segmentation::ES::set_reg(GDT_SELECTORS.1.data);
        x86_64::instructions::segmentation::SS::set_reg(GDT_SELECTORS.1.data);
        x86_64::instructions::tables::load_tss(GDT_SELECTORS.1.tss);
    }

    IDT.load();
}

extern "x86-interrupt" fn breakpoint(stack: InterruptStackFrame) {
    println!("Breakpoint!\n{:#?}", stack);
}

extern "x86-interrupt" fn double_fault(stack: InterruptStackFrame, error: u64) -> ! {
    panic!("Double Fault (Error Code {}):\n{:#?}", error, stack);
}

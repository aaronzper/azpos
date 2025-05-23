use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::{registers::segmentation::Segment, structures::{gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector}, idt::InterruptDescriptorTable, tss::TaskStateSegment}, VirtAddr};

use crate::{devices::pic::{self, PICInterrupt}, memory::PAGE_SIZE};

mod handlers;

/// Runs a function without interrupting
pub use x86_64::instructions::interrupts::without_interrupts;

const INT_STACK_SIZE: usize = PAGE_SIZE as usize * 4;
const INT_STACK_INDEX: usize = 0;

// Needs to be mut so that the stack isn't put into RODATA
static mut INT_STACK: [u8; INT_STACK_SIZE] = [0; INT_STACK_SIZE];

pub struct GDTSegments {
    gdt: GlobalDescriptorTable,
    pub code: SegmentSelector,
    pub data: SegmentSelector,
    tss: SegmentSelector,
}

static PIC: Mutex<pic::PIC> = Mutex::new(pic::PIC::new());

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        idt.breakpoint.set_handler_fn(handlers::breakpoint);
        idt[PICInterrupt::Timer as u8].set_handler_fn(handlers::timer);
        idt[PICInterrupt::Keyboard as u8].set_handler_fn(handlers::keyboard);

        unsafe {
            idt.double_fault.set_handler_fn(handlers::double_fault)
                .set_stack_index(INT_STACK_INDEX as u16);
        }

        idt
    };

    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();

        let stack_beg = VirtAddr::from_ptr(&raw mut INT_STACK);
        let stack_end = stack_beg + INT_STACK_SIZE as u64;
        tss.interrupt_stack_table[INT_STACK_INDEX] = stack_end;

        tss
    };

    pub static ref GDT: GDTSegments = {
        let mut gdt = GlobalDescriptorTable::new();
        
        let code = gdt.append(Descriptor::kernel_code_segment());
        let data = gdt.append(Descriptor::kernel_data_segment());
        let tss = gdt.append(Descriptor::tss_segment(&TSS));

        GDTSegments { gdt, code, data, tss }
    };
}

/// Enables hardware interrupts
fn enable_interrupts() {
    x86_64::instructions::interrupts::enable();
}

/// Put the CPU to sleep until next interrupt
pub fn wait() {
    x86_64::instructions::interrupts::enable_and_hlt();
}

/// Initializes interrupts by loading the IDT
pub fn init_interrupts() {
    GDT.gdt.load();
    unsafe {
        x86_64::instructions::segmentation::CS::set_reg(GDT.code);
        x86_64::instructions::segmentation::DS::set_reg(GDT.data);
        x86_64::instructions::segmentation::ES::set_reg(GDT.data);
        x86_64::instructions::segmentation::SS::set_reg(GDT.data);
        x86_64::instructions::tables::load_tss(GDT.tss);
    }

    IDT.load();

    PIC.lock().initialize();
    enable_interrupts();
}

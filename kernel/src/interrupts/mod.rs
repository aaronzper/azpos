use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::{registers::segmentation::Segment, structures::{gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector}, idt::InterruptDescriptorTable, tss::TaskStateSegment}, VirtAddr};

use crate::{devices::pic::{self, PICInterrupt}, memory::PAGE_SIZE};

mod handlers;

const STACK_SIZE: usize = PAGE_SIZE as usize * 4;

// Needs to be mut so that the stack isn't put into RODATA
static mut DOUBLE_FAULT_STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
const DOUBLE_FAULT_STACK_I: usize = 0;

static mut PIC_STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
const PIC_STACK_I: usize = 1;

fn stack_end(stack: *const [u8; STACK_SIZE]) -> VirtAddr {
    let top = stack as usize + STACK_SIZE;
    VirtAddr::new(top as u64)
}

pub struct GDTSegments {
    gdt: GlobalDescriptorTable,
    pub code: SegmentSelector,
    pub data: SegmentSelector,
    pub user_code: SegmentSelector,
    pub user_data: SegmentSelector,
    tss: SegmentSelector,
}

static PIC: Mutex<pic::PIC> = Mutex::new(pic::PIC::new());

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        idt.breakpoint.set_handler_fn(handlers::breakpoint);

        unsafe {
            idt.double_fault.set_handler_fn(handlers::double_fault)
                .set_stack_index(DOUBLE_FAULT_STACK_I as u16);

            idt[PICInterrupt::Keyboard as u8].set_handler_fn(handlers::keyboard)
                .set_stack_index(PIC_STACK_I as u16);

            let timer_addr = VirtAddr::new(handlers::timer as u64);
            idt[PICInterrupt::Timer as u8].set_handler_addr(timer_addr)
                .set_stack_index(PIC_STACK_I as u16);
        }

        idt
    };

    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();

        tss.interrupt_stack_table[DOUBLE_FAULT_STACK_I] = 
            stack_end(&raw const DOUBLE_FAULT_STACK);
        tss.interrupt_stack_table[PIC_STACK_I] = 
            stack_end(&raw const PIC_STACK);

        tss
    };

    pub static ref GDT: GDTSegments = {
        let mut gdt = GlobalDescriptorTable::new();
        
        // Segments need to be put into GDT in this order for STAR to work right
        let code = gdt.append(Descriptor::kernel_code_segment());
        let data = gdt.append(Descriptor::kernel_data_segment());
        let user_data = gdt.append(Descriptor::user_data_segment());
        let user_code = gdt.append(Descriptor::user_code_segment());

        let tss = gdt.append(Descriptor::tss_segment(&TSS));

        GDTSegments { gdt, code, data, user_code, user_data, tss }
    };
}

/// Enables hardware interrupts
pub fn enable_interrupts() {
    x86_64::instructions::interrupts::enable();
}

/// Disables hardware interrupts
pub fn disable_interrupts() {
    x86_64::instructions::interrupts::disable();
}

/// Returns whether interrupts are enabled
pub fn interrupts_enabled() -> bool {
    x86_64::instructions::interrupts::are_enabled()
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

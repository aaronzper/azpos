use x86_64::{registers::rflags::RFlags, structures::idt::InterruptStackFrameValue, VirtAddr};

use crate::interrupts::GDT;

#[derive(Clone, Debug)]
#[repr(C)]
/// Stores the state of the CPU (registers, etc)
pub struct CpuState {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,

    // Stores RIP and RSP
    pub int_stack: InterruptStackFrameValue,
}

impl CpuState {
    /// Creates a new state with a given stack pointer, and entrypoint, and 
    /// argument (as passed to the entrypoint in `RDI`). All other registers are
    /// set to 0.
    pub fn new(stack: VirtAddr, entry: VirtAddr, arg: u64) -> CpuState {
        CpuState {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rdi: arg,
            rsi: 0,
            rbp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            int_stack: InterruptStackFrameValue::new(
                entry,
                GDT.code,
                RFlags::INTERRUPT_FLAG,
                stack,
                GDT.data
            ),
        }
    }
}

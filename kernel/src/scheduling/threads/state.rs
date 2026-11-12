use x86_64::{registers::rflags::RFlags, structures::idt::InterruptStackFrameValue, VirtAddr};

use crate::{interrupts::GDT, memory::user::USER_END_ADDR};

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

    /// Returns whether the state is currently at a userland RIP
    pub fn is_user(&self) -> bool {
        self.int_stack.instruction_pointer.as_u64() <= USER_END_ADDR
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktest::*;

    ktest! {
        /// `new` sets the stack, the entry, the argument and the interrupt flag.
        fn new_state_sets_entry_stack_and_argument() {
            let stack = VirtAddr::new(0xFFFF_FF00_0000_0000);
            let entry = VirtAddr::new(0xFFFF_8000_0010_0000);
            let state = CpuState::new(stack, entry, 42);
            check_eq!(state.rdi, 42);
            check_eq!(state.rax, 0);
            check_eq!(state.int_stack.instruction_pointer, entry);
            check_eq!(state.int_stack.stack_pointer, stack);
            check!(state.int_stack.cpu_flags.contains(RFlags::INTERRUPT_FLAG));
        }

        /// A low instruction pointer is user code. A high one is kernel code.
        fn is_user_follows_the_instruction_pointer() {
            let stack = VirtAddr::new(0x1000);
            let user = CpuState::new(stack, VirtAddr::new(0x40_0000), 0);
            let kernel = CpuState::new(stack, VirtAddr::new(0xFFFF_8000_0000_0000), 0);
            check!(user.is_user());
            check!(!kernel.is_user());
        }
    }
}

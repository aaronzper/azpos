use x86_64::{registers::rflags::RFlags, structures::idt::InterruptStackFrameValue, VirtAddr};

use crate::{interrupts::GDT, memory::stacks::{KThreadStack, KERNEL_STACK_ALLOCATOR}};

use super::state::CpuState;

/// A thread identifier
pub type ThreadID = u32;

/// An individual, scheduable thread of execution
pub struct Thread {
    /// The thread state
    pub state: CpuState,
    /// The thread's entrypoint. Used by the scheduler to start it
    entry_point: VirtAddr,
    /// How many times the thread has been scheduled. If 0, the thread hasn't
    /// been started
    runs: usize,
    /// The thread's stack, if its a kernel thread (user stacks are handled in
    /// user space)
    stack: Option<KThreadStack>,
}

impl Thread {
    /// Creates a new kthread that will start executing at the given entrypoint
    pub fn new_kthread(entry_point: fn() -> ()) -> Thread {
        let stack = KERNEL_STACK_ALLOCATOR.lock().alloc_stack()
            .expect("Out of memory");

        let entry_ptr = VirtAddr::from_ptr(entry_point as *const ());

        Thread {
            state: CpuState::new(stack.top(), entry_ptr),
            entry_point: entry_ptr,
            runs: 0,
            stack: Some(stack),
        }
    }

    /// Starts and hands control over to the thread. Panics if the thread has
    /// already been started. Unsafe cause duh.
    pub unsafe fn start(&mut self) -> ! {
        if self.started() {
            panic!("Thread has already been started!");
        }
        self.runs += 1;

        let stack = self.state.int_stack.clone();

        unsafe {
            stack.iretq()
        }
    }

    /// Returns whether the thread  has been started
    pub fn started(&self) -> bool {
        self.runs != 0
    }

    /// Returns the number of times this thread has been scheduled
    pub fn runs(&self) -> usize {
        self.runs
    }

    /// Increments the `runs` counter
    pub fn add_run(&mut self) {
        self.runs += 1
    }
}

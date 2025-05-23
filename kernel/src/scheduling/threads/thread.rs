use crate::memory::stacks::{KThreadStack, KERNEL_STACK_ALLOCATOR};


/// A thread identifier
pub type ThreadID = u32;

/// An individual, scheduable thread of execution
pub struct Thread {
    /// The thread stack pointer. All other thread state is stored on the stack
    stack_ptr: VirtAddr,
    /// The thread's stack, if its a kernel thread (user stacks are handled in
    /// user space)
    stack: Option<KThreadStack>,
}

impl Thread {
    pub fn new_kthread() -> Thread {
        let stack = KERNEL_STACK_ALLOCATOR.lock().alloc_stack()
            .expect("Out of memory");

        Thread {
            stack_ptr: stack.top(),
            stack: Some(stack),
        }
    }
}

use x86_64::{registers::rflags::RFlags, structures::idt::InterruptStackFrameValue, VirtAddr};

use crate::{interrupts::GDT, memory::stacks::{KThreadStack, KERNEL_STACK_ALLOCATOR}};

/// A thread identifier
pub type ThreadID = u32;

/// An individual, scheduable thread of execution
pub struct Thread {
    /// The thread stack pointer. All other thread state is stored on the stack
    stack_ptr: VirtAddr,
    /// The thread's entrypoint. Used by the scheduler to start it
    entry_point: VirtAddr,
    /// Whether the thread has been started or not
    started: bool,
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
            stack_ptr: stack.top(),
            entry_point: entry_ptr,
            started: false,
            stack: Some(stack),
        }
    }


    /// Starts and hands control over to the thread. Panics if the thread has
    /// already been started. Unsafe cause duh.
    pub unsafe fn start(&mut self) -> ! {
        if self.started {
            panic!("Thread has already been started!");
        }
        self.started = true;

        let stack_frame = InterruptStackFrameValue::new(
            self.entry_point(),
            GDT.code,
            RFlags::INTERRUPT_FLAG,
            self.stack_ptr(),
            GDT.data,
        );

        unsafe {
            stack_frame.iretq()
        }
    }

    /// Returns the address of the thread's entrypoint
    pub fn entry_point(&self) -> VirtAddr {
        self.entry_point
    }

    /// Returns the thread's most recent known stack pointer
    ///
    /// If the thread is currently running this may not be current
    pub fn stack_ptr(&self) -> VirtAddr {
        self.stack_ptr
    }
}

use alloc::boxed::Box;
use x86_64::VirtAddr;

use crate::{memory::stacks::{KThreadStack, KERNEL_STACK_ALLOCATOR}, processes::ProcessID};

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
    /// The thread's stack, if its a kernel thread, or kernel stack, if its a
    /// user thread
    kstack: KThreadStack,
    /// The thread's process, if its a user thread
    process: Option<ProcessID>,
}

impl Thread {
    /// Creates a new thread that will start executing at the given entrypoint.
    /// Optionally takes a process ID, if its a user thread.
    pub fn new_thread<F, T>(entrypoint: F, proc: Option<ProcessID>) -> Self
        where F: FnOnce() -> T + Send + 'static,
              T: Send + 'static {

        let kstack = KERNEL_STACK_ALLOCATOR.lock().alloc_stack()
            .expect("Out of memory");

        let runner_ptr = VirtAddr::from_ptr(run_thread::<F, T> as *const ());

        // Create and immediately leak a box with the entrypoint. It'll get
        // re-boxed and freed in `run_thread`.
        let entrypoint_ref = Box::leak(Box::new(entrypoint));
        let entrypoint_ptr = (entrypoint_ref as *mut F) as u64;

        Self {
            state: CpuState::new(kstack.top(), runner_ptr, entrypoint_ptr),
            entry_point: runner_ptr,
            runs: 0,
            kstack,
            process: proc,
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

extern "C" fn run_thread<F, T>(entrypoint: &mut F) -> !
    where F: FnOnce() -> T + Send + 'static,
          T: Send + 'static {

    let boxed = unsafe { Box::from_raw(entrypoint) };
    boxed();
    println!("Thread finished!");
    loop {} // TODO: Thread exit
}

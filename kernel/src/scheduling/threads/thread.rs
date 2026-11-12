use alloc::boxed::Box;
use x86_64::VirtAddr;
use crate::{memory::stacks::{KThreadStack, KERNEL_STACK_ALLOCATOR}, processes::ProcessID, scheduling::thread_exit};
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

    /// Returns the thread's PID, if it has one
    pub fn proccess(&self) -> Option<ProcessID> {
        self.process
    }

    /// Returns the address of the top of the thread's kernel stack
    pub fn stack_top(&self) -> VirtAddr {
        self.kstack.top()
    }
}

extern "C" fn run_thread<F, T>(entrypoint: &mut F) -> !
    where F: FnOnce() -> T + Send + 'static,
          T: Send + 'static {

    let boxed = unsafe { Box::from_raw(entrypoint) };
    boxed();

    unsafe { thread_exit(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;
    use crate::ktest::*;

    ktest! {
        /// A new thread has not run and starts at its runner on its stack.
        fn new_thread_has_not_started() {
            let thread = Thread::new_thread(|| {}, None);
            check!(!thread.started());
            check_eq!(thread.runs(), 0);
            check!(thread.proccess().is_none());
            check_eq!(thread.state.int_stack.instruction_pointer, thread.entry_point);
            check_eq!(thread.state.int_stack.stack_pointer, thread.stack_top());
            check!(!thread.state.is_user());
        }

        /// `add_run` counts the runs, and the first run starts the thread.
        fn add_run_starts_the_thread() {
            let mut thread = Thread::new_thread(|| {}, None);
            thread.add_run();
            thread.add_run();
            check!(thread.started());
            check_eq!(thread.runs(), 2);
        }

        /// A thread that never runs drops its closure when the thread drops.
        /// Today `new_thread` leaks the box of the closure.
        #[known_bug(67)]
        fn dropped_thread_drops_its_closure() {
            let shared = Arc::new(());
            let owned = Arc::clone(&shared);
            let thread = Thread::new_thread(move || { let _keep = &owned; }, None);
            drop(thread);
            check_eq!(Arc::strong_count(&shared), 1);
        }
    }
}

//! The kernel test harness.
//!
//! A test build (`cargo test -p azpos-kernel`) boots like the normal kernel.
//! The thread that starts the init process is replaced by `main_thread`,
//! which runs each `ktest!` case and reports the results on the serial port.
//! The host harness (`azpos::session`) reads the reports.
//!
//! Rules for a test case:
//! - Use the check macros, not `assert!`. A panic stops the kernel, and the
//!   host must boot QEMU again.
//! - Leave global kernel state as you found it. All cases of one boot share
//!   the scheduler, the heap and the process table.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};

use libtestkit::{protocol::SUITE_KERNEL, run_suite, Args, Case};
use spin::Once;

use crate::{
    filesystem::fat::FATFilesystem,
    scheduling::{thread_yield, threads::{Thread, ThreadID}, SCHEDULER},
};

/// Functions that stop QEMU
mod exit;
/// Reader for the QEMU firmware configuration device
mod fw_cfg;
/// A block device in memory
mod ramdisk;

pub use exit::on_panic;
#[allow(unused_imports)]
pub use ramdisk::{RamDisk, RAM_BLOCK_SIZE};
// The prelude for test modules: `use crate::ktest::*;`
#[allow(unused_imports)]
pub use libtestkit::{check, check_eq, check_matches, check_ne, fail, ktest, OrFail};

/// The `fw_cfg` file that holds the argument text. The host sets it with the
/// QEMU option `-fw_cfg name=opt/azpos/ktest,string=...`.
const ARGS_FILE: &str = "opt/azpos/ktest";

/// The path of the user test program on the test disk
const USER_TEST_PROGRAM: &str = "/programs/systest.exe";

/// The entry point of the thread that runs the tests
pub fn main_thread() {
    // The compiler generates this function. It calls `run` with all cases.
    crate::ktest_main();
}

/// Runs the kernel suite, then starts the user suite or stops QEMU
pub fn run(cases: &[&Case]) {
    let args_bytes = fw_cfg::read_file(ARGS_FILE).unwrap_or_default();
    let args_text = core::str::from_utf8(&args_bytes).unwrap_or("");
    let args = Args::parse(args_text);

    if args.kernel {
        run_suite(SUITE_KERNEL, cases, &args, &mut |line| println!("{line}"));
    }

    if args.user {
        // STOPGAP: a process cannot exit, so the kernel cannot wait for the
        // user suite. The host stops QEMU when it reads `done user`. When
        // `exit` exists (GitHub #4), wait for the process here and call
        // `exit::exit_qemu`.
        crate::spawn_from_fs(root_fs(), USER_TEST_PROGRAM, "systest");
    } else {
        exit::exit_qemu(exit::ExitCode::Success);
    }
}

struct RootFs(FATFilesystem<'static>);

// The test cases run one at a time, and the file system is read-only.
unsafe impl Send for RootFs {}
unsafe impl Sync for RootFs {}

static ROOT_FS: Once<RootFs> = Once::new();

/// Returns the root file system of the test disk. The first call mounts it.
///
/// The host puts the fixture files of `azpos::fixtures` in the directory
/// `/tests` of this disk.
pub fn root_fs() -> &'static FATFilesystem<'static> {
    &ROOT_FS.call_once(|| RootFs(crate::mount_root_fs())).0
}

/// Runs `f` in a new kernel thread and returns when the thread is complete
pub fn run_in_thread<F: FnOnce() + Send + 'static>(f: F) {
    let done = Arc::new(AtomicBool::new(false));
    let done_in_thread = Arc::clone(&done);

    let thread = Thread::new_thread(move || {
        f();
        done_in_thread.store(true, Ordering::SeqCst);
    }, None);
    SCHEDULER.lock().add_thread(thread);

    while !done.load(Ordering::SeqCst) {
        thread_yield();
    }
}

/// The maximum number of yields in one call of `wait_until`
const WAIT_YIELDS: usize = 100_000;

/// Starts `f` in a new kernel thread and returns its thread ID.
///
/// This function does not wait for the thread.
#[allow(dead_code)]
pub fn spawn_thread<F: FnOnce() + Send + 'static>(f: F) -> ThreadID {
    let thread = Thread::new_thread(f, None);
    SCHEDULER.lock().add_thread(thread)
}

/// Yields until `cond` is true.
///
/// Returns `false` if `cond` is still false after `WAIT_YIELDS` yields.
#[allow(dead_code)]
pub fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..WAIT_YIELDS {
        if cond() {
            return true;
        }
        thread_yield();
    }
    cond()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicUsize;

    ktest! {
        /// The harness itself: a case can start a thread and wait for it.
        fn run_in_thread_waits_for_the_thread() {
            let count = Arc::new(AtomicUsize::new(0));
            let count_in_thread = Arc::clone(&count);

            run_in_thread(move || {
                for _ in 0..3 {
                    count_in_thread.fetch_add(1, Ordering::SeqCst);
                    thread_yield();
                }
            });

            check_eq!(count.load(Ordering::SeqCst), 3);
        }

        /// The harness itself: QEMU has the device that holds the arguments.
        fn fw_cfg_device_is_present() {
            check!(fw_cfg::present());
        }
    }
}

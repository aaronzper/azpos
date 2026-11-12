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
    devices::storage::BlockDevice,
    filesystem::fat::FATFilesystem,
    scheduling::{thread_yield, threads::{Thread, ThreadID}, SCHEDULER},
};

/// Functions that stop QEMU
mod exit;
/// Reader for the QEMU firmware configuration device
mod fw_cfg;
/// A block device in memory
mod ramdisk;
/// A builder for small ELF files
mod elf;

pub use exit::on_panic;
#[allow(unused_imports)]
pub use ramdisk::{RamDisk, RAM_BLOCK_SIZE};
#[allow(unused_imports)]
pub use elf::{minimal_elf, Segment};
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
        // Kernel cases leave processes in `PROCESSES`.
        // `spawn_proc` then needs a big stack (see `processes::tests::spawn`).
        on_big_stack(|| crate::spawn_from_fs(root_fs(), USER_TEST_PROGRAM, "systest"));
    } else {
        exit::exit_qemu(exit::ExitCode::Success);
    }
}

struct RootFs(FATFilesystem<'static>, *mut dyn BlockDevice);

// The test cases run one at a time, and the file system is read-only.
unsafe impl Send for RootFs {}
unsafe impl Sync for RootFs {}

static ROOT_FS: Once<RootFs> = Once::new();

/// Mounts the test disk on the first call
fn root_disk() -> &'static RootFs {
    // The mount step needs more stack than a case has left
    ROOT_FS.call_once(|| on_big_stack(|| {
        let (fs, spare) = crate::mount_root_fs_and_spare();
        RootFs(fs, spare)
    }))
}

/// Returns the root file system of the test disk. The first call mounts it.
///
/// The host puts the fixture files of `azpos::fixtures` in the directory
/// `/tests` of this disk.
pub fn root_fs() -> &'static FATFilesystem<'static> {
    &root_disk().0
}

/// Returns partition 1 of the test disk, the boot partition.
///
/// The kernel does not use this partition after boot.
/// A case can write to it, because QEMU discards the writes at the end of the boot.
/// Use the value in one case at a time, and do not keep it after the case.
#[allow(dead_code)]
pub fn spare_partition() -> &'static mut dyn BlockDevice {
    // SAFETY: the cases run one at a time, so only one reference is in use.
    unsafe { &mut *root_disk().1 }
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

/// The size of the stack that `on_big_stack` uses
const BIG_STACK_SIZE: usize = 64 * 1024;

/// Runs `f` on a stack of `BIG_STACK_SIZE` bytes on the heap, in the current thread.
///
/// A kernel thread has a stack of 16 KiB.
/// Some kernel calls need more than that in a debug build, for example `spawn_proc`.
#[allow(dead_code)]
pub fn on_big_stack<R, F: FnOnce() -> R>(f: F) -> R {
    use alloc::{boxed::Box, vec};
    use core::ffi::c_void;

    /// Calls the closure that `data` points to
    extern "C" fn trampoline(data: *mut c_void) {
        let call = unsafe { &mut *(data as *mut &mut dyn FnMut()) };
        call();
    }

    let stack: Box<[u8]> = vec![0u8; BIG_STACK_SIZE].into_boxed_slice();
    // The System V ABI needs a stack top with 16-byte alignment
    let top = (stack.as_ptr() as usize + BIG_STACK_SIZE) & !0xF;

    let mut f = Some(f);
    let mut result = None;
    let mut call = || result = f.take().map(|f| f());
    let mut call_dyn: &mut dyn FnMut() = &mut call;
    let data = &mut call_dyn as *mut &mut dyn FnMut() as *mut c_void;

    // R12 is callee-saved, so it keeps the old stack pointer across the call
    unsafe {
        core::arch::asm!(
            "mov r12, rsp",
            "mov rsp, rsi",
            "call rax",
            "mov rsp, r12",
            in("rdi") data,
            in("rsi") top,
            in("rax") trampoline as usize,
            out("r12") _,
            clobber_abi("C"),
        );
    }

    drop(stack);
    result.expect("the closure did not run")
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

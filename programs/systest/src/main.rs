//! The user test program. It runs the test cases of the syscall API in one
//! process and reports each result on the kernel log.
//!
//! The host harness (`azpos::session`) reads the reports from the serial
//! port. The program runs in two places:
//! - On the test kernel, after the kernel suite (`cargo test -p azpos-kernel`).
//! - On the production kernel, as the init process
//!   (`cargo test -p azpos --test e2e`).
//!
//! Rules for a test case:
//! - Use the check macros. A panic stops the process, and the host must boot
//!   QEMU again.
//! - Use the raw `sys_*` functions and close each resource that you open.

#![no_std]
#![no_main]

use libsystem::alloc::vec::Vec;
use libtestkit::{protocol::SUITE_USER, run_suite, Args, Case};

/// Reads the arguments that the host wrote into the ELF file
mod args;
/// Writes protocol records to the kernel log
mod output;

/// Cases for `list_devices` and the blob resource that it returns
mod devices;
/// Cases for `get_logger` and the logger resource
mod logger;
/// Cases for the resource calls on an invalid resource ID
mod resource;
/// Cases for `yield`
mod yield_;

/// The case list of each module. The order sets the index of each case.
/// Add each new module here.
static MODULES: &[&[Case]] = &[
    yield_::CASES,
    logger::CASES,
    resource::CASES,
    devices::CASES,
];

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    let mut args_buf = [0; libtestkit::stopgap::ARGS_CAPACITY];
    let args = Args::parse(args::read(&mut args_buf));

    let cases: Vec<&Case> = MODULES.iter().flat_map(|module| module.iter()).collect();

    let mut output = output::Output::new();
    run_suite(SUITE_USER, &cases, &args, &mut |line| output.line(line));

    // STOPGAP: a process cannot exit (GitHub #4). `main` returns and
    // `_start` spins. The host stops QEMU when it reads `done user`.
}

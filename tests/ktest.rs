//! The kernel tests (`ktest!` cases in the kernel crate), then the user test
//! program (`programs/systest`), on the test build of the kernel.
//!
//! This test has no libtest harness. `cargo ktest ...` passes its arguments
//! to `main`.

use std::process::exit;

use azpos::{cli::Options, session::{self, Target}, test_kernel};

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            exit(2);
        }
    };

    let kernel_elf = match test_kernel::build() {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            exit(1);
        }
    };

    println!("\n==== ktest: kernel suite and user suite on the test kernel ====");
    let target = Target::TestKernel { kernel_elf };
    exit(session::run_and_report(&target, &options));
}

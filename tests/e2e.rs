//! End-to-end test: the user test program (`programs/systest`) on the
//! production kernel.
//!
//! This test has no libtest harness. `cargo e2e ...` passes its arguments to
//! `main`.

use std::process::exit;

use azpos::{cli::Options, session::{self, Target}};

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            exit(2);
        }
    };

    println!("\n==== e2e: user suite on the production kernel ====");
    exit(session::run_and_report(&Target::Production, &options));
}

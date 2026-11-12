#![no_std]

extern crate alloc;

/// Arguments that the host passes to a guest test runner
mod args;
/// The test case type and its failure type
mod case;
/// Macros that declare test cases and check conditions
mod macros;
/// The line protocol between a guest test runner and the host
pub mod protocol;
/// The guest test runner
mod run;
/// Temporary mechanisms. Each one names the ticket that removes it.
pub mod stopgap;

#[doc(hidden)]
pub use alloc::format as __format;

pub use args::Args;
pub use case::{Case, Failure, OrFail, TestResult};
pub use run::run_suite;

//! STOPGAP: a process has no arguments (GitHub #5, #41). The host writes the
//! argument text into `SYSTEST_ARGS` in the ELF file. See
//! `libtestkit::stopgap`.

use libtestkit::stopgap::{read_args, ArgsBlock, ARGS_CAPACITY};

#[used]
#[unsafe(no_mangle)]
static mut SYSTEST_ARGS: ArgsBlock = ArgsBlock::EMPTY;

/// Copies the argument text into `buf` and returns it
pub fn read(buf: &mut [u8; ARGS_CAPACITY]) -> &str {
    unsafe { read_args(&raw const SYSTEST_ARGS, buf) }
}

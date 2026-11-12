use libsystem::syscalls::sys_yield;
use libtestkit::{cases, check};

cases! {
    /// `yield` returns to the caller, also when no other thread can run.
    fn yield_returns() {
        for _ in 0..3 {
            sys_yield();
        }
        check!(true);
    }
}

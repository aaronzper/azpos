//! Spins forever. It makes no syscall.

#![no_std]
#![no_main]

testprogs::bare_program! {
    loop {}
}

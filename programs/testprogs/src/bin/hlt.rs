//! Runs the instruction `hlt` in user mode. The CPU gives a general protection fault.

#![no_std]
#![no_main]

testprogs::bare_program! {
    unsafe { core::arch::asm!("hlt") };
}

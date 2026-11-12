//! Runs the instruction `ud2`. The CPU gives an invalid opcode fault.

#![no_std]
#![no_main]

testprogs::bare_program! {
    unsafe { core::arch::asm!("ud2") };
}

//! Reads the address `0x10`. No page maps it, so the CPU gives a page fault.

#![no_std]
#![no_main]

testprogs::bare_program! {
    unsafe { core::ptr::read_volatile(0x10 as *const u8) };
}

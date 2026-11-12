//! Writes to the first kernel address. The CPU gives a page fault.

#![no_std]
#![no_main]

testprogs::bare_program! {
    unsafe { core::ptr::write_volatile(0xFFFF_8000_0000_0000 as *mut u8, 1) };
}

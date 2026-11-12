//! Divides by zero. The CPU gives a divide error.

#![no_std]
#![no_main]

testprogs::bare_program! {
    unsafe {
        core::arch::asm!(
            "xor ecx, ecx",
            "xor edx, edx",
            "mov eax, 1",
            "div ecx",
            out("eax") _, out("ecx") _, out("edx") _,
        )
    };
}

use core::arch::asm;

use crate::Syscall;

pub extern "C" fn make_syscall(syscall: Syscall) -> u64 {
    let rax: u64;
    unsafe {
        asm!(
            "push rbx",
            "push r12",
            "push r13",
            "push r14",
            "push r15",
            "syscall",
            "pop r15",
            "pop r14",
            "pop r13",
            "pop r12",
            "pop rbx",
            out("rax") rax,
        )
    };

    rax
}

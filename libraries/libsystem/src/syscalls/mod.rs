use core::arch::asm;
use libsci::Syscall;

extern "C" fn make_syscall(syscall: Syscall, arg1: u64, arg2: u64) -> u64 {
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

pub fn print(msg: &str) {
    let ptr = msg.as_ptr() as u64;
    let len = msg.len() as u64;
    make_syscall(Syscall::Print, ptr, len);
}

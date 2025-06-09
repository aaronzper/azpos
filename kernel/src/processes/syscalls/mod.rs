use core::arch::global_asm;

use x86_64::{registers::{control::{Efer, EferFlags}, model_specific::{LStar, Star}}, VirtAddr};

use crate::interrupts::GDT;

global_asm!(include_str!("entrypoint.s"));
unsafe extern "C" {
    pub fn syscall_entry();
}

/// Sets up the SYSCALL/SYSRET instructions by writing to STAR & LSTAR
pub fn init_syscalls() {
    Star::write(GDT.user_code, GDT.user_data, GDT.code, GDT.data).unwrap();
    LStar::write(VirtAddr::from_ptr(syscall_entry as *const ()));

    let flags = EferFlags::SYSTEM_CALL_EXTENSIONS | Efer::read();
    unsafe { Efer::write(flags) };
}

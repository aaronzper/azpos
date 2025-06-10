use core::arch::global_asm;
use alloc::slice;
use libsci::{resources::{result_to_rax, ResourceID}, Syscall};
use x86_64::{registers::{control::{Efer, EferFlags}, model_specific::{GsBase, LStar, Star}}, VirtAddr};
use crate::interrupts::GDT;

/// System call handlers
mod handlers;
/// Various `Resource` implementations for kernel things that are exposed to
/// users
mod resources;

/// Pointer to kstack of the currently running user thread. Undefined if the
/// current thread isnt a user thread.
static mut CURRENT_USER_KSTACK_PTR: u64 = 0;

global_asm!(include_str!("entrypoint.s"));
unsafe extern "C" {
    pub fn syscall_entry();
}

#[unsafe(no_mangle)]
extern "C" fn syscall(syscall: Syscall, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    match syscall {
        Syscall::Yield => handlers::sys_yield(),

        Syscall::Close =>
            result_to_rax(handlers::sys_close(arg1 as ResourceID)),

        Syscall::GetLogger => handlers::sys_get_logger() as i64,

        Syscall::Read => {
            let rid = arg1 as ResourceID;
            let ptr = arg2 as *mut u8;
            let len = arg3 as usize;
            let buf = unsafe { slice::from_raw_parts_mut(ptr, len) };

            result_to_rax(handlers::sys_read(rid, buf))
        },


        Syscall::Write => {
            let rid = arg1 as ResourceID;
            let ptr = arg2 as *const u8;
            let len = arg3 as usize;
            let buf = unsafe { slice::from_raw_parts(ptr, len) };
            
            result_to_rax(handlers::sys_write(rid, buf))
        },

        Syscall::Seek => handlers::sys_seek(),

        _ => panic!("Invalid syscall type"),
    }
}

pub fn set_syscall_stack(top: VirtAddr) {
    let top_u64 = top.as_u64();
    unsafe { CURRENT_USER_KSTACK_PTR = top_u64; }
}

/// Sets up the SYSCALL/SYSRET instructions
pub fn init_syscalls() {
    // Write user and kernel segments to STAR
    Star::write(GDT.user_code, GDT.user_data, GDT.code, GDT.data).unwrap();
    // Write syscall handlers address to LSTAR
    LStar::write(VirtAddr::from_ptr(syscall_entry as *const ()));
    // Write user kstack address to GS_Base (will be swapped when entering userland)
    GsBase::write(VirtAddr::from_ptr(&raw const CURRENT_USER_KSTACK_PTR));

    // Enable syscall extension
    let flags = EferFlags::SYSTEM_CALL_EXTENSIONS | Efer::read();
    unsafe { Efer::write(flags) };
}

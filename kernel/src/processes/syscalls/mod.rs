use core::arch::global_asm;
use alloc::{borrow::ToOwned, slice, string::String};
use libsyscall::Syscall;
use x86_64::{registers::{control::{Efer, EferFlags}, model_specific::{GsBase, KernelGsBase, LStar, Star}}, VirtAddr};
use crate::{interrupts::GDT, scheduling::{thread_yield, SCHEDULER}};

/// Pointer to kstack of the currently running user thread. Undefined if the
/// current thread isnt a user thread.
static mut CURRENT_USER_KSTACK_PTR: u64 = 0;

global_asm!(include_str!("entrypoint.s"));
unsafe extern "C" {
    pub fn syscall_entry();
}

#[unsafe(no_mangle)]
extern "C" fn syscall(syscall: Syscall, arg1: u64, arg2: u64) -> u64 {
    match syscall {
        Syscall::Yield => { thread_yield(); 0 },
        Syscall::TestPing => {
            let sched = SCHEDULER.lock();
            let tid = sched.currently_running().unwrap();
            let pid = sched.get_thread(tid).unwrap().proccess().unwrap();
            println!("Syscall from PID {pid} with args {arg1} & {arg2}!");
            613
        },
        Syscall::Print => {
            let ptr = arg1 as *const ();
            let len = arg2 as usize;
            let slice = unsafe {
                slice::from_raw_parts(ptr as *const u8, len)
            };
            match String::from_utf8(slice.to_owned()) {
                Ok(s) => {
                    let sched = SCHEDULER.lock();
                    let tid = sched.currently_running().unwrap();
                    let pid = sched.get_thread(tid).unwrap().proccess().unwrap();
                    println!("PID {pid} says: {s}");
                }
                Err(_) => return 1,
            }

            0
        }
        _ => panic!("Invalid syscall type"),
    }
}

pub fn set_syscall_stack(top: VirtAddr) {
    let top_u64 = top.as_u64();
    unsafe { CURRENT_USER_KSTACK_PTR = top_u64; }
}

/// Sets up the SYSCALL/SYSRET instructions by writing to STAR & LSTAR
pub fn init_syscalls() {
    Star::write(GDT.user_code, GDT.user_data, GDT.code, GDT.data).unwrap();
    LStar::write(VirtAddr::from_ptr(syscall_entry as *const ()));
    GsBase::write(VirtAddr::from_ptr(&raw const CURRENT_USER_KSTACK_PTR));

    let flags = EferFlags::SYSTEM_CALL_EXTENSIONS | Efer::read();
    unsafe { Efer::write(flags) };
}

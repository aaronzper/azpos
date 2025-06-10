use core::arch::global_asm;
use alloc::{borrow::ToOwned, boxed::Box, slice, string::String};
use libsci::{resources::ResourceID, Syscall};
use resources::LoggerResource;
use x86_64::{registers::{control::{Efer, EferFlags}, model_specific::{GsBase, KernelGsBase, LStar, Star}}, VirtAddr};
use crate::{interrupts::GDT, scheduling::{thread_yield, SCHEDULER}};
use super::PROCESSES;

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
extern "C" fn syscall(syscall: Syscall, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    match syscall {
        Syscall::Yield => { 
            thread_yield();
            0
        },

        Syscall::Close => {
            let pid = SCHEDULER.lock().current_proc().unwrap();
            let mut procs = PROCESSES.lock();
            let p = procs.get_proc_mut(pid).unwrap();
            let rid = arg1 as ResourceID;
            p.resources.remove(&rid);

            0
        }

        Syscall::GetLogger => {
            let pid = SCHEDULER.lock().current_proc().unwrap();
            let mut procs = PROCESSES.lock();
            let p = procs.get_proc_mut(pid).unwrap();

            // TODO: Assign RID dynamically
            let rid = 123;
            p.resources.insert(rid, Box::new(LoggerResource::new()));

            rid as u64
        }

        Syscall::Read => todo!(),

        Syscall::Write => {
            let pid = SCHEDULER.lock().current_proc().unwrap();
            let mut procs = PROCESSES.lock();
            let p = procs.get_proc_mut(pid).unwrap();

            let rid = arg1 as ResourceID;
            let resource = match p.resources.get_mut(&rid) {
                Some(r) => r,
                None => return -1i64 as u64,
            };

            let ptr = arg2 as *const u8;
            let len = arg3 as usize;
            let buf = unsafe { slice::from_raw_parts(ptr, len) };

            resource.write(buf).unwrap()
        }

        Syscall::Seek => todo!(),

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

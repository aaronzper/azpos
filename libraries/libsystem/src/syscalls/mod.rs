use core::arch::global_asm;

use libsci::{resources::{rax_to_result, ResourceID, ResourceResult}, Syscall};

mod resource;
pub use resource::SystemResource;

global_asm!(include_str!("make_syscall.s"));
unsafe extern "C" {
    fn make_syscall(syscall: Syscall, arg1: u64, arg2: u64, arg3: u64) -> i64;
}

fn make_syscall_no_args(syscall: Syscall) -> i64 {
    unsafe { make_syscall(syscall, 0, 0, 0) }
}

fn make_syscall_1_arg(syscall: Syscall, arg: u64) -> i64 {
    unsafe { make_syscall(syscall, arg, 0, 0) }
}

fn make_syscall_2_args(syscall: Syscall, arg1: u64, arg2: u64) -> i64 {
    unsafe { make_syscall(syscall, arg1, arg2, 0) }
}

fn make_syscall_3_args(syscall: Syscall, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    unsafe { make_syscall(syscall, arg1, arg2, arg3) }
}

pub fn sys_yield() {
    make_syscall_no_args(Syscall::Yield);
}

pub fn sys_get_logger() -> ResourceID {
    make_syscall_no_args(Syscall::GetLogger) as ResourceID
}

pub fn sys_read(rid: ResourceID, buf: &mut [u8]) -> ResourceResult {
    let ptr = buf.as_mut_ptr() as u64;
    let len = buf.len() as u64;
    let rax = make_syscall_3_args(Syscall::Read, rid as u64, ptr, len);
    rax_to_result(rax)
}

pub fn sys_write(rid: ResourceID, buf: &[u8]) -> ResourceResult {
    let ptr = buf.as_ptr() as u64;
    let len = buf.len() as u64;
    let rax = make_syscall_3_args(Syscall::Write, rid as u64, ptr, len);
    rax_to_result(rax)
}

pub fn sys_close(rid: ResourceID) -> ResourceResult { 
    let rax = make_syscall_1_arg(Syscall::Close, rid as u64);
    rax_to_result(rax)
}

pub fn sys_seek(rid: ResourceID, offset: usize) -> ResourceResult {
    let rax = make_syscall_2_args(Syscall::Seek, rid as u64, offset as u64);
    rax_to_result(rax)
}

pub fn sys_list_devices() -> ResourceID {
    make_syscall_no_args(Syscall::ListDevices) as ResourceID
}

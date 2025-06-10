use alloc::boxed::Box;
use libsci::resources::{ResourceError, ResourceID, ResourceResult};
use crate::{processes::PROCESSES, scheduling::{thread_yield, SCHEDULER}};
use super::resources::LoggerResource;

pub fn sys_yield() -> u64 {
    thread_yield();
    0
}

pub fn sys_close(rid: ResourceID) -> u64 {
    let pid = SCHEDULER.lock().current_proc().unwrap();
    let mut procs = PROCESSES.lock();
    let p = procs.get_proc_mut(pid).unwrap();
    p.resources.remove(&rid);

    0
}

pub fn sys_get_logger() -> u64 {
    let pid = SCHEDULER.lock().current_proc().unwrap();
    let mut procs = PROCESSES.lock();
    let p = procs.get_proc_mut(pid).unwrap();

    // TODO: Assign RID dynamically
    let rid = 123;
    p.resources.insert(rid, Box::new(LoggerResource::new()));

    rid as u64
}

pub fn sys_read() -> u64 {
    todo!()
}

pub fn sys_write(rid: ResourceID, buf: &[u8]) -> ResourceResult<u64> {
    let pid = SCHEDULER.lock().current_proc().unwrap();
    let mut procs = PROCESSES.lock();
    let p = procs.get_proc_mut(pid).unwrap();

    let resource = match p.resources.get_mut(&rid) {
        Some(r) => r,
        None => return Err(ResourceError::ResourceNotFound),
    };

    resource.write(buf)
}

pub fn sys_seek() -> u64 {
    todo!()
}

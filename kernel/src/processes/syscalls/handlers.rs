use alloc::{borrow::ToOwned, boxed::Box, vec};
use libsci::resources::{ResourceError, ResourceID, ResourceResult};
use crate::{processes::PROCESSES, scheduling::{thread_yield, SCHEDULER}};
use super::resources::{BlobResource, LoggerResource};

pub fn sys_yield() -> i64 {
    thread_yield();
    0
}

pub fn sys_close(rid: ResourceID) -> ResourceResult {
    let pid = SCHEDULER.lock().current_proc().unwrap();
    let mut procs = PROCESSES.lock();
    let p = match procs.get_entry_mut(pid) {
        Some(p) => p,
        None => return Err(ResourceError::ResourceNotFound),
    };

    p.resources.remove_entry(rid);
    Ok(0)
}

pub fn sys_get_logger() -> ResourceID {
    let logger = Box::new(LoggerResource::new());

    let pid = SCHEDULER.lock().current_proc().unwrap();
    let mut procs = PROCESSES.lock();
    let p = procs.get_entry_mut(pid).unwrap();

    p.resources.add_entry(logger)
}

pub fn sys_read(rid: ResourceID, buf: &mut [u8]) -> ResourceResult {
    let pid = SCHEDULER.lock().current_proc().unwrap();
    let mut procs = PROCESSES.lock();
    let p = procs.get_entry_mut(pid).unwrap();

    let resource = match p.resources.get_entry_mut(rid) {
        Some(r) => r,
        None => return Err(ResourceError::ResourceNotFound),
    };

    resource.read(buf)
}

pub fn sys_write(rid: ResourceID, buf: &[u8]) -> ResourceResult {
    let pid = SCHEDULER.lock().current_proc().unwrap();
    let mut procs = PROCESSES.lock();
    let p = procs.get_entry_mut(pid).unwrap();

    let resource = match p.resources.get_entry_mut(rid) {
        Some(r) => r,
        None => return Err(ResourceError::ResourceNotFound),
    };

    resource.write(buf)
}

pub fn sys_seek() -> i64 {
    todo!()
}

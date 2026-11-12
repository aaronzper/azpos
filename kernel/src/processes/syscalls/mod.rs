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

        Syscall::Seek =>
            result_to_rax(handlers::sys_seek(arg1 as ResourceID, arg2 as usize)),

        Syscall::ListDevices => handlers::sys_list_devices() as i64,

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ktest::*;
    use crate::memory::{resolve_virt_addr, user::USER_END_ADDR, PAGE_SIZE};
    use crate::processes::tests::{spawn_test_program, wait_for_process_to_run};
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, Ordering};
    use x86_64::registers::{model_specific::SFMask, rflags::RFlags};

    ktest! {
        fn efer_enables_the_syscall_instruction() {
            check!(Efer::read().contains(EferFlags::SYSTEM_CALL_EXTENSIONS));
        }

        fn lstar_holds_the_address_of_syscall_entry() {
            check_eq!(LStar::read(), VirtAddr::from_ptr(syscall_entry as *const ()));
        }

        /// STAR holds the user and kernel segments of the GDT.
        /// `syscall` loads the kernel segments, and `sysretq` loads the user segments.
        fn star_holds_the_segments_of_the_gdt() {
            let (user_code, user_data, kernel_code, kernel_data) = Star::read();
            check_eq!(user_code, GDT.user_code);
            check_eq!(user_data, GDT.user_data);
            check_eq!(kernel_code, GDT.code);
            check_eq!(kernel_data, GDT.data);
        }

        /// SFMASK clears the interrupt, direction and trap flags on syscall entry (GitHub #30).
        /// Today `init_syscalls` does not write SFMASK.
        #[known_bug(30)]
        fn sfmask_clears_the_interrupt_direction_and_trap_flags() {
            let mask = SFMask::read();
            check!(mask.contains(RFlags::INTERRUPT_FLAG), "SFMASK = {mask:?}");
            check!(mask.contains(RFlags::DIRECTION_FLAG), "SFMASK = {mask:?}");
            check!(mask.contains(RFlags::TRAP_FLAG), "SFMASK = {mask:?}");
        }

        /// A kernel thread does not see the memory of a process (GitHub #40).
        /// The address is in the stack of the process.
        #[known_bug(40)]
        fn a_kernel_thread_does_not_see_the_memory_of_a_process() {
            let pid = spawn_test_program("yield").or_fail("spawn yield")?;
            wait_for_process_to_run(pid)?;

            let addr = VirtAddr::new(USER_END_ADDR - 2 * PAGE_SIZE);
            let mapped = Arc::new(AtomicBool::new(true));
            let mapped_in_thread = Arc::clone(&mapped);
            run_in_thread(move || {
                mapped_in_thread.store(resolve_virt_addr(addr).is_some(), Ordering::SeqCst);
            });
            check!(!mapped.load(Ordering::SeqCst), "{addr:?} is mapped in a kernel thread");
        }
    }
}

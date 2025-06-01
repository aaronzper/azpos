use core::alloc::Layout;
use alloc::alloc::alloc;
use x86_64::{structures::paging::{frame::PhysFrameRange, FrameDeallocator, Mapper, Page, PageTableFlags}, PhysAddr, VirtAddr};
use crate::memory::{paging::current_pt, PAGE_ALLOCATOR};

use super::{paging::SizedPage, resolve_phys_addr, PAGE_SIZE};

/// Allocates a block of physical memory that:
/// - Is page-aligned, and padded until the start of the next page after the
///   allocation
/// - Occupies continuous physical frames
/// - Is *not* cacheable (cache disabled on all pages)
///
/// Returns `None` if out of memory, there are no good frames to use, etc.
/// Otherwise returns a pointer to the corresponding virtual address of the
/// allocation, and the physical frames used for the allocation
///
/// Unsafe because the caller is responsible for freeing this
pub unsafe fn alloc_mmio_block<T>(len: usize) -> Option<(*mut T, PhysFrameRange)> {
    let n_pages = (len + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;

    // Remap the allocation to contiguous physical frames w/ cache disabled
    let mut lock = PAGE_ALLOCATOR.lock();
    let page_alloc = lock.as_mut().unwrap();
    let mut pt = current_pt();

    let frames = page_alloc.alloc_range(n_pages)?;
    let start_pa = frames.start.start_address();
    let start_va = resolve_phys_addr(start_pa).unwrap();
    
    let flags = PageTableFlags::PRESENT 
        | PageTableFlags::WRITABLE 
        | PageTableFlags::NO_CACHE;
    let mut va = start_va;
    for _ in frames {
        let page = SizedPage::from_start_address(va).unwrap();
        let flusher = unsafe { pt.update_flags(page, flags).unwrap() };
        flusher.flush();
        va += PAGE_SIZE;
    }

    Some((start_va.as_mut_ptr(), frames))
}

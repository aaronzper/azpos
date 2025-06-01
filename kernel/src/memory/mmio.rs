use x86_64::structures::paging::frame::PhysFrameRange;
use crate::memory::PAGE_ALLOCATOR;
use super::{resolve_phys_addr, PAGE_SIZE};

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

    let mut lock = PAGE_ALLOCATOR.lock();
    let page_alloc = lock.as_mut().unwrap();

    let frames = page_alloc.alloc_range(n_pages)?;
    let start_pa = frames.start.start_address();
    let start_va = resolve_phys_addr(start_pa).unwrap();
    let ptr = start_va.as_mut_ptr();

    Some((ptr, frames))
}

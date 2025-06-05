use bitvec::{field::BitField, order::Lsb0, view::BitView};
use core::any::type_name;
use x86_64::structures::paging::{frame::PhysFrameRange, mapper::TranslateResult, Mapper, PageTableFlags, Translate};
use crate::memory::PAGE_ALLOCATOR;
use super::{paging::{current_pt, downsize_pages, SizedPage}, resolve_phys_addr, PAGE_SIZE};

/// Reads the given bitfield from the given raw value. Useful for parsing
/// MMIO structures. Panics if cant fit into output type.
pub fn read_bitfield<I: BitView, O: TryFrom<u64>>(raw: I, from: usize, to: usize) -> O {
    let bits = raw.view_bits::<Lsb0>().get(from..to).unwrap();
    let out: u64 = bits.load_le();
    match out.try_into() {
        Ok(x) => x,
        Err(_) =>
            panic!("Couldn't fit bitfield value {} into {}",
                out, type_name::<O>()),
    }
}

/// Writes the given bitfield to the given raw value. Useful for parsing
/// MMIO structures. 
pub fn write_bitfield<I: BitView, O: Into<u64>>(raw: &mut I, from: usize, to: usize, value: O) {
    let bits = raw.view_bits_mut::<Lsb0>().get_mut(from..to).unwrap();
    bits.store_le(value.into());
}

/// Allocates a block of physical memory that:
/// - Is page-aligned, and padded until the start of the next page after the
///   allocation
/// - Occupies continuous physical frames
/// - Is *not* cacheable (cache disabled on all pages)
/// - Is all zeroed
/// - Is `len` bytes long, before the padding (see first bullet)
///
/// Returns `None` if out of memory, there are no good frames to use, etc.
/// Otherwise returns a pointer to the corresponding virtual address of the
/// allocation, and the physical frames used for the allocation
///
/// Unsafe because the caller is responsible for freeing this
pub unsafe fn alloc_mmio_block<T>(len: usize) -> Option<(*mut T, PhysFrameRange)> {
    let n_pages = (len + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;

    let frames = {
        let mut lock = PAGE_ALLOCATOR.lock();
        let page_alloc = lock.as_mut().unwrap();

        page_alloc.alloc_range(n_pages)?
    };

    let start_pa = frames.start.start_address();
    let start_va = resolve_phys_addr(start_pa).unwrap();
    let end_va = start_va + len as u64;

    unsafe { downsize_pages(start_va, len as u64) };

    // No-cache the allocation
    let mut pt = current_pt();
    let mut va = start_va;
    while va < end_va {
        let flags = match pt.translate(va) {
            TranslateResult::Mapped { frame: _, offset: _, flags } => flags,
            x => panic!("Invalid translation: {:?}", x),
        };

        let new_flags = flags | PageTableFlags::NO_CACHE;
        let page = SizedPage::from_start_address(va).unwrap();
        unsafe { pt.update_flags(page, new_flags) }.unwrap().flush();

        va += PAGE_SIZE;
    }

    // Zero the allocation
    // We can do this in 64-bit steps since its garunteed to be page-aligned,
    // so we wont overrun the end or something
    for va in (start_va..end_va).step_by(size_of::<u64>()) {
        unsafe { *(va.as_mut_ptr() as *mut u64) = 0 };
    }

    let ptr = start_va.as_mut_ptr();

    Some((ptr, frames))
}

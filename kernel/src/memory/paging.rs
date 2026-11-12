use core::usize;

use bootloader_api::info::{MemoryRegion, MemoryRegionKind, MemoryRegions};
use x86_64::{registers::control::Cr3, structures::paging::{frame::{PhysFrameRange, PhysFrameRangeInclusive}, mapper::MapToError, page_table::PageTableEntry, FrameAllocator, FrameDeallocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size2MiB, Size4KiB}, PhysAddr, VirtAddr};

use super::{physical_map_addr, resolve_phys_addr, PAGE_ALLOCATOR, PAGE_SIZE};

/// The type used for the physical page reference count
pub type PageRefCount = u8;
/// The type of the page size (used by the `x86_64` crate
pub type PageSizeType = Size4KiB;
pub type SizedPhysFrame = PhysFrame<PageSizeType>;
pub type SizedPage = Page<PageSizeType>;

/// Returns the currently active top-level Page Table
pub fn current_pt() -> OffsetPageTable<'static> {
    let (pt_pa, _) = Cr3::read();
    let pt_va = resolve_phys_addr(pt_pa.start_address()).unwrap();

    unsafe {
        let pt = &mut *pt_va.as_mut_ptr();
        OffsetPageTable::new(pt, physical_map_addr().into())
    }
}

/// Yields the page table pointer to by some page table entry. Panics if the
/// given entry isnt present.
///
/// Unsafe because called needs to ensure the address in the PT entry is a valid
/// Page Table
pub unsafe fn pt_from_pt_entry<'a>(entry: &PageTableEntry) -> &'a mut PageTable {
    if entry.flags() & PageTableFlags::PRESENT == PageTableFlags::empty() {
        panic!("Page table entry is not present!");
    }

    let pa = entry.addr();
    let va = resolve_phys_addr(pa).unwrap();
    let ptr = va.as_mut_ptr() as *mut PageTable;
    unsafe { &mut *ptr }
}

/// Returns all frames that are fully contained within the given region.
///
/// Returns `None` if there are no such frames.
fn get_frames_from_region(region: &MemoryRegion) -> 
    Option<PhysFrameRangeInclusive<PageSizeType>> {

    let mut start = region.start;
    let mut end = region.end;

    let start_mod = start % PAGE_SIZE;
    if start_mod != 0 {
        start += PAGE_SIZE - start_mod;
    }

    let end_mod = end % PAGE_SIZE;
    if end_mod != 0 {
        end -= end_mod;
    }


    if start == end {
        return None;
    }

    let first_pa = PhysAddr::new(start);
    let last_pa = PhysAddr::new(end - 1);

    let first_frame: SizedPhysFrame = PhysFrame::containing_address(first_pa);
    let last_frame: SizedPhysFrame = PhysFrame::containing_address(last_pa);

    Some(PhysFrame::range_inclusive(first_frame, last_frame))
}

struct BasicAllocator<'a> {
    p_regions: &'a MemoryRegions,
    alloced: usize,
}

impl<'a> BasicAllocator<'a> {
    fn get_usable_frames(&self) -> impl Iterator<Item = PhysFrame> + use<'a> {
        let usable = self.p_regions.iter()
            .filter(|r| r.kind == MemoryRegionKind::Usable)
            .filter_map(get_frames_from_region)
            .flat_map(|frames| frames)
            .skip(self.alloced);

        usable
    }

    fn new(p_regions: &'a MemoryRegions) -> BasicAllocator<'a> {
        BasicAllocator {
            p_regions,
            alloced: 0
        }
    }
}

unsafe impl<'a> FrameAllocator<PageSizeType> for BasicAllocator<'a> {
    fn allocate_frame(&mut self) -> Option<SizedPhysFrame> {
        match self.get_usable_frames().next() {
            Some(frame) => {
                self.alloced += 1;
                Some(frame)
            },
            None => None
        }
    }
}

fn frame_to_index(frame: PhysFrame) -> usize {
    (frame.start_address().as_u64() / PAGE_SIZE) as usize
}

fn index_to_frame(index: usize) -> PhysFrame {
    let pa = PhysAddr::new(index as u64 * PAGE_SIZE);
    PhysFrame::containing_address(pa)
}

/// Allocates physical pages (aka frames) via a global reference count.
pub struct PageAllocator<'a> {
    frame_refcounts: &'a mut [PageRefCount],
    next_alloc: usize,
    avail_bytes: usize,
}

/// Provides statistics on the physical page allocator
#[derive(Debug)]
pub struct PhysMemoryStats {
    /// The total number of bytes in the system (may be lower than actual due to
    /// BIOS/UEFI reserved regions)
    pub total_bytes: usize,
    /// The total number of bytes available to be allocated
    pub avail_bytes: usize,
}

impl<'a> PageAllocator<'a> {
    /// Provides statistics on physical memory usage
    pub fn get_stats(&self) -> PhysMemoryStats {
        PhysMemoryStats {
            total_bytes: self.frame_refcounts.len() * PAGE_SIZE as usize,
            avail_bytes: self.avail_bytes,
        }
    }

    /// Creates and initializes the Page allocator given a ref-count array and 
    /// memory region list. `unsafe` since the virtual address space used by the
    /// refcounts must be free and usable.
    pub unsafe fn new(
        frame_refcounts: &'a mut [PageRefCount],
        p_regions: &MemoryRegions,
    ) -> PageAllocator<'a> {

        let mut basic_alloc = BasicAllocator::new(p_regions);
        let mut pt = current_pt();

        // Allocate & map the space needed for the refcounts
        let mut va = VirtAddr::from_ptr(frame_refcounts);
        let end = VirtAddr::from_ptr(frame_refcounts.as_mut_ptr_range().end);
        while va < end {
            let v_page: Page<PageSizeType> = Page::containing_address(va);
            let p_frame = basic_alloc.allocate_frame().expect("Out of memory");
            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

            let map_result = unsafe {
                pt.map_to(v_page, p_frame, flags, &mut basic_alloc)
            };

            match map_result {
                Ok(f) => f.flush(),
                Err(e) => match e {
                    MapToError::FrameAllocationFailed => 
                        panic!("{:?} (Out of memory)", e),
                    MapToError::ParentEntryHugePage => (),  // These dont
                    MapToError::PageAlreadyMapped(_) => (), // matter
                }
            };

            va = v_page.start_address() + PAGE_SIZE;
        }

        let allocator = PageAllocator { 
            avail_bytes: frame_refcounts.len() * PAGE_SIZE as usize,
            frame_refcounts, 
            next_alloc: 0,
        };

        let mut usable_frames = basic_alloc.get_usable_frames().peekable();
        
        for (i, rc) in allocator.frame_refcounts.iter_mut().enumerate() {
            let frame = index_to_frame(i);
            match usable_frames.peek() {
                Some(next_frame) => if frame == *next_frame {
                    *rc = 0;
                    usable_frames.next();
                    continue;
                },
                None => ()
            };
            
            *rc = 1;
        }

        allocator
    }

    /// Allocates the given page to any physical frame.
    ///
    /// Does nothing (including updating flags) if the page is already mapped.
    pub fn alloc_page(&mut self, page: SizedPage, flags: PageTableFlags) {
        let mut pt = current_pt();

        if pt.translate_page(page).is_ok() {
            return;
        }

        let p_frame = self.allocate_frame().expect("Out of memory");

        let flush = unsafe {
            pt.map_to(page, p_frame, flags, self).unwrap()
        };
        flush.flush();
    }

    /// Allocates a contiguous range of physical frames of the given
    /// size, if one exists
    pub fn alloc_range(&mut self, n_frames: usize) -> Option<PhysFrameRange> {
        // TODO: Optimize
        let start_index = self.frame_refcounts
            .windows(n_frames)
            .enumerate()
            .find(|(_, block)| block.iter().all(|c| *c == 0))
            .map(|(index, _)| index)?;
        let end_index = start_index + n_frames;

        for i in start_index..end_index {
            assert!(self.frame_refcounts[i] == 0);
            self.frame_refcounts[i] = 1;
        }
        self.next_alloc = end_index;
        self.avail_bytes -= PAGE_SIZE as usize * n_frames;

        let from_frame = index_to_frame(start_index);
        let to_frame = index_to_frame(end_index);
        Some(PhysFrame::range(from_frame, to_frame))
    }
}

unsafe impl<'a> FrameAllocator<PageSizeType> for PageAllocator<'a> {
    /// Allocates a given frame
    fn allocate_frame(&mut self) -> Option<SizedPhysFrame> {
        let index = self.frame_refcounts.iter()
            .enumerate()
            .skip(self.next_alloc)
            .find(|(_, count)| **count == 0);

        match index {
            Some((i, _)) => {
                self.next_alloc = i + 1;
                self.frame_refcounts[i] = 1;
                self.avail_bytes -= PAGE_SIZE as usize;
                Some(index_to_frame(i))
            },
            None => None
        }
    }
}

impl<'a> FrameDeallocator<PageSizeType> for PageAllocator<'a> {
    /// Frees a given frame
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame) {
        let i = frame_to_index(frame);
        if self.frame_refcounts[i] == 0 {
            panic!("Physical frame double free at {:?}", frame);
        }

        self.frame_refcounts[i] -= 1;

        if self.frame_refcounts[i] == 0 {
            self.avail_bytes += PAGE_SIZE as usize;
        }
    }
}

/// Deallocates the given frame, decrementing its reference count by 1
pub fn dealloc_frame(frame: PhysFrame) {
    let mut lock = PAGE_ALLOCATOR.lock();
    unsafe { lock.as_mut().unwrap().deallocate_frame(frame); }
}

/// Remaps the physical memory map in that given range into "normal" (4KiB) 
/// sized pages, instead of the large pages provided by the bootloader. 
/// This gives us more granular control in determining which flags to use
/// over which addresses.
//
/// Unsafe because playing with page tables is scary (boo!)
pub unsafe fn downsize_pages(start_va: VirtAddr, len: u64) {
    let end_va = start_va + len;

    let start_big_page = Page::<Size2MiB>::containing_address(start_va);
    let end_big_page = Page::<Size2MiB>::containing_address(end_va - 1);
    let big_page_range = Page::range_inclusive(start_big_page, end_big_page);
    
    let pt = current_pt();
    let mut lock = PAGE_ALLOCATOR.lock();
    let page_alloc = lock.as_mut().unwrap();

    for big_page in big_page_range {
        let l2pt = unsafe {
            let l4pt = pt.level_4_table();
            let l3pt = pt_from_pt_entry(&l4pt[big_page.p4_index()]);
            pt_from_pt_entry(&l3pt[big_page.p3_index()])
        };

        let p2_entry = &mut l2pt[big_page.p2_index()];

        let old_flags = p2_entry.flags();

        // This already isn't a huge page, skip!
        if old_flags & PageTableFlags::HUGE_PAGE == PageTableFlags::empty() {
            continue;
        }

        let l1pt_frame: SizedPhysFrame = page_alloc.allocate_frame().unwrap();
        let l1pt_va = resolve_phys_addr(l1pt_frame.start_address()).unwrap();
        let l1pt: &mut PageTable = unsafe { 
            &mut *(l1pt_va.as_mut_ptr() as *mut PageTable)
        };

        let flags = old_flags & !PageTableFlags::HUGE_PAGE;

        let mut pa = p2_entry.addr();
        for entry in l1pt.iter_mut() {
            entry.set_addr(pa, flags);
            pa += PAGE_SIZE;
        }

        p2_entry.set_addr(l1pt_frame.start_address(), flags);
    }

    x86_64::instructions::tlb::flush_all();
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use crate::ktest::*;

    fn region(start: u64, end: u64) -> MemoryRegion {
        MemoryRegion { start, end, kind: MemoryRegionKind::Usable }
    }

    fn frame_count(start: u64, end: u64) -> usize {
        get_frames_from_region(&region(start, end)).map_or(0, |r| r.count())
    }

    ktest! {
        /// An aligned region gives each frame in it.
        fn aligned_region_gives_each_frame() {
            let frames = get_frames_from_region(&region(0x1000, 0x4000))
                .or_fail("get_frames_from_region")?;
            check_eq!(frames.start.start_address().as_u64(), 0x1000);
            check_eq!(frames.end.start_address().as_u64(), 0x3000);
            check_eq!(frames.count(), 3);
        }

        /// An unaligned start rounds up. An unaligned end rounds down.
        fn unaligned_region_rounds_to_full_frames() {
            let frames = get_frames_from_region(&region(0x1001, 0x4FFF))
                .or_fail("get_frames_from_region")?;
            check_eq!(frames.start.start_address().as_u64(), 0x2000);
            check_eq!(frames.end.start_address().as_u64(), 0x3000);
            check_eq!(frames.count(), 2);
        }

        /// A region inside one page contains no full frame.
        fn region_inside_one_page_gives_no_frame() {
            check_eq!(frame_count(0x1100, 0x1F00), 0);
            check_eq!(frame_count(0x1100, 0x2000), 0);
            check_eq!(frame_count(0x1000, 0x1000), 0);
        }

        /// The rustdoc says that a region with no full frame gives `None`.
        /// Today a start that rounds past the end gives `Some` of an empty range.
        #[known_bug(unfiled)]
        fn region_inside_one_page_gives_none() {
            check!(get_frames_from_region(&region(0x1000, 0x1000)).is_none());
            check!(get_frames_from_region(&region(0x1100, 0x1F00)).is_none());
        }

        /// `frame_to_index` and `index_to_frame` are inverse.
        fn frame_index_conversions_are_inverse() {
            for i in [0, 1, 255, 0x12345] {
                check_eq!(frame_to_index(index_to_frame(i)), i);
            }
            let frame = PhysFrame::containing_address(PhysAddr::new(0x7000));
            check_eq!(index_to_frame(frame_to_index(frame)), frame);
        }

        /// The statistics never show more free bytes than total bytes.
        fn avail_bytes_is_at_most_total_bytes() {
            let stats = PAGE_ALLOCATOR.lock().as_ref().or_fail("allocator")?.get_stats();
            check!(stats.avail_bytes <= stats.total_bytes, "{:?}", stats);
            check!(stats.avail_bytes > 0);
        }

        /// `allocate_frame` takes one page. `dealloc_frame` gives it back.
        fn allocate_and_dealloc_frame_change_avail_bytes_by_one_page() {
            let mut lock = PAGE_ALLOCATOR.lock();
            let alloc = lock.as_mut().or_fail("allocator")?;
            let before = alloc.get_stats().avail_bytes;
            let frame = alloc.allocate_frame().or_fail("allocate_frame")?;
            let after_alloc = alloc.get_stats().avail_bytes;
            drop(lock);

            dealloc_frame(frame);
            let after_free = PAGE_ALLOCATOR.lock().as_ref().or_fail("allocator")?
                .get_stats().avail_bytes;

            check_eq!(after_alloc, before - PAGE_SIZE as usize);
            check_eq!(after_free, before);
        }

        /// Two calls of `allocate_frame` give two different frames.
        fn two_allocations_give_two_frames() {
            let mut lock = PAGE_ALLOCATOR.lock();
            let alloc = lock.as_mut().or_fail("allocator")?;
            let a = alloc.allocate_frame().or_fail("first allocate_frame")?;
            let b = alloc.allocate_frame().or_fail("second allocate_frame")?;
            unsafe {
                alloc.deallocate_frame(a);
                alloc.deallocate_frame(b);
            }
            check_ne!(a, b);
        }

        /// `alloc_range(4)` gives 4 frames in a row and marks each one as used.
        fn alloc_range_gives_contiguous_frames() {
            let mut lock = PAGE_ALLOCATOR.lock();
            let alloc = lock.as_mut().or_fail("allocator")?;
            let before = alloc.get_stats().avail_bytes;
            let range = alloc.alloc_range(4).or_fail("alloc_range")?;
            let after = alloc.get_stats().avail_bytes;
            let all_used = range.into_iter()
                .all(|f| alloc.frame_refcounts[frame_to_index(f)] == 1);
            for frame in range {
                unsafe { alloc.deallocate_frame(frame); }
            }
            let restored = alloc.get_stats().avail_bytes;

            check_eq!(range.count(), 4);
            check!(all_used);
            check_eq!(after, before - 4 * PAGE_SIZE as usize);
            check_eq!(restored, before);
        }

        /// A range longer than physical memory gives `None`.
        fn alloc_range_longer_than_memory_gives_none() {
            let mut lock = PAGE_ALLOCATOR.lock();
            let alloc = lock.as_mut().or_fail("allocator")?;
            let n = alloc.frame_refcounts.len() + 1;
            check!(alloc.alloc_range(n).is_none());
        }

        /// `alloc_page` of a mapped page keeps the frame and takes no memory.
        fn alloc_page_of_a_mapped_page_keeps_the_frame() {
            let boxed = Box::new(0u64);
            let page = SizedPage::containing_address(VirtAddr::from_ptr(&*boxed));
            let before = current_pt().translate_page(page).ok().or_fail("translate")?;

            let mut lock = PAGE_ALLOCATOR.lock();
            let alloc = lock.as_mut().or_fail("allocator")?;
            let avail_before = alloc.get_stats().avail_bytes;
            alloc.alloc_page(page, PageTableFlags::PRESENT | PageTableFlags::WRITABLE);
            let avail_after = alloc.get_stats().avail_bytes;
            drop(lock);

            let after = current_pt().translate_page(page).ok().or_fail("translate")?;
            check_eq!(after, before);
            check_eq!(avail_after, avail_before);
        }

        /// A second free of one frame is a kernel error.
        #[should_panic("Physical frame double free")]
        fn double_dealloc_frame_panics() {
            let frame = PAGE_ALLOCATOR.lock().as_mut().or_fail("allocator")?
                .allocate_frame().or_fail("allocate_frame")?;
            dealloc_frame(frame);
            dealloc_frame(frame);
        }

        /// A freed frame is free for the next allocation.
        /// Today `next_alloc` stays above it, so the allocator skips it.
        #[known_bug(unfiled)]
        fn freed_frame_can_be_allocated_again() {
            let mut lock = PAGE_ALLOCATOR.lock();
            let alloc = lock.as_mut().or_fail("allocator")?;
            let frame = alloc.allocate_frame().or_fail("allocate_frame")?;
            unsafe { alloc.deallocate_frame(frame); }
            let next_alloc = alloc.next_alloc;
            check!(next_alloc <= frame_to_index(frame),
                "next_alloc {} is above the freed index {}",
                next_alloc, frame_to_index(frame));
        }

        /// `alloc_range(0)` gives `None` or an empty range.
        /// Today `windows(0)` panics.
        #[known_bug(unfiled, crash)]
        fn alloc_range_of_zero_frames_is_empty() {
            let mut lock = PAGE_ALLOCATOR.lock();
            let alloc = lock.as_mut().or_fail("allocator")?;
            let range = alloc.alloc_range(0);
            check!(range.map_or(true, |r| r.is_empty()));
        }
    }
}

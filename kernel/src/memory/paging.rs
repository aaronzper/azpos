use core::usize;

use bootloader_api::info::{MemoryRegion, MemoryRegionKind, MemoryRegions};
use x86_64::{registers::control::Cr3, structures::paging::{frame::{PhysFrameRange, PhysFrameRangeInclusive}, mapper::MapToError, FrameAllocator, FrameDeallocator, Mapper, OffsetPageTable, Page, PageTableFlags, PhysFrame, Size4KiB}, PhysAddr, VirtAddr};

use super::{physical_map_addr, resolve_phys_addr, PAGE_SIZE};

/// The type used for the physical page reference count
pub type PageRefCount = u8;
/// The type of the page size (used by the `x86_64` crate
pub type PageSizeType = Size4KiB;
type SizedPhysFrame = PhysFrame<PageSizeType>;
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

use core::alloc;

use bootloader_api::info::{MemoryRegion, MemoryRegionKind, MemoryRegions};
use spin::Mutex;
use x86_64::{registers::control::Cr3, structures::paging::{frame::PhysFrameRangeInclusive, FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB}, PhysAddr, VirtAddr};

use super::{physical_map_addr, resolve_phys_addr};

pub type PageRefCount = u8;
type PageSizeType = Size4KiB;
type SizedPhysFrame = PhysFrame<PageSizeType>;

pub const PAGE_SIZE: u64 = 0x1000;

static PAGE_ALLOC: Mutex<Option<PageAllocator>> = Mutex::new(None);

pub fn current_pt() -> OffsetPageTable<'static> {
    let (pt_pa, _) = Cr3::read();
    let pt_va = resolve_phys_addr(pt_pa.start_address()).unwrap();

    unsafe {
        let pt = &mut *pt_va.as_mut_ptr();
        OffsetPageTable::new(pt, physical_map_addr().into())
    }
}

fn get_frames_from_region(region: &MemoryRegion) -> 
    PhysFrameRangeInclusive<PageSizeType> {

    let first_pa = PhysAddr::new(region.start);
    let last_pa = PhysAddr::new(region.end - 1);

    let first_frame: SizedPhysFrame = PhysFrame::containing_address(first_pa);
    let last_frame: SizedPhysFrame = PhysFrame::containing_address(last_pa);

    PhysFrame::range_inclusive(first_frame, last_frame)
}

struct BasicAllocator<'a> {
    p_regions: &'a MemoryRegions,
    alloced: usize,
}

impl<'a> BasicAllocator<'a> {
    pub fn new(p_regions: &'a MemoryRegions) -> BasicAllocator<'a> {
        BasicAllocator {
            p_regions,
            alloced: 0
        }
    }
}

unsafe impl<'a> FrameAllocator<PageSizeType> for BasicAllocator<'a> {
    fn allocate_frame(&mut self) -> Option<SizedPhysFrame> {
        let mut free_frames = self.p_regions.iter()
            .filter(|r| r.kind == MemoryRegionKind::Usable)
            .flat_map(get_frames_from_region)
            .skip(self.alloced);

        match free_frames.next() {
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

pub struct PageAllocator<'a> {
    frame_refcounts: &'a mut [PageRefCount],
    next_alloc: usize,
    avail_bytes: usize,
}

#[derive(Debug)]
pub struct PhysMemoryStats {
    pub total_bytes: usize,
    pub avail_bytes: usize,
}

impl<'a> PageAllocator<'a> {
    pub fn get_stats(&self) -> PhysMemoryStats {
        PhysMemoryStats {
            total_bytes: self.frame_refcounts.len() * PAGE_SIZE as usize,
            avail_bytes: self.avail_bytes,
        }
    }

    fn set_refcount(&mut self, frame: PhysFrame, count: PageRefCount) {
        let i = frame_to_index(frame);
        self.frame_refcounts[i] = count;
    }

    pub unsafe fn new(
        frame_refcounts: &'a mut [PageRefCount],
        p_regions: &MemoryRegions,
        n_used_regions: usize,
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

            let flush = unsafe {
                pt.map_to(v_page, p_frame, flags, &mut basic_alloc).unwrap()
            };
            flush.flush();

            va = v_page.start_address() + PAGE_SIZE;
        }

        let mut allocator = PageAllocator { 
            avail_bytes: frame_refcounts.len() * PAGE_SIZE as usize,
            frame_refcounts, 
            next_alloc: 0,
        };

        let reserved_frames = p_regions.iter()
            .take(n_used_regions)
            .filter(|r| r.kind != MemoryRegionKind::Usable)
            .flat_map(get_frames_from_region);

        let allocated_frames = p_regions.iter()
            .take(n_used_regions)
            .filter(|r| r.kind == MemoryRegionKind::Usable)
            .flat_map(get_frames_from_region)
            .take(basic_alloc.alloced);
        
        for p in reserved_frames.chain(allocated_frames) {
            allocator.set_refcount(p, 1);
            allocator.avail_bytes -= PAGE_SIZE as usize;
        }

        allocator
    }

    pub fn free_frame(&mut self, frame: PhysFrame) {
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

unsafe impl<'a> FrameAllocator<PageSizeType> for PageAllocator<'a> {
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

use core::slice;

use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use paging::{PageAllocator, PageRefCount, PAGE_SIZE};
use x86_64::{structures::paging::{FrameAllocator, PhysFrame}, PhysAddr, VirtAddr};

mod paging;

pub const KERNEL_START_ADDR: u64 = 0xFFFF_8000_0000_0000;

static mut PHYS_MAP_ADDR: VirtAddr = VirtAddr::new(0);
static mut PHYS_SIZE: u64 = 0;
static mut HEAP_START: VirtAddr = VirtAddr::new(0);

pub fn get_phys_size() -> u64 {
    unsafe {
        PHYS_SIZE
    }
}

fn physical_map_addr() -> VirtAddr {
    unsafe {
        PHYS_MAP_ADDR
    }
}

fn resolve_phys_addr(pa: PhysAddr) -> Option<VirtAddr> {
    let sz = get_phys_size();
    if pa.as_u64() >= sz {
        panic!(
            "Physical address {:#X} is past the physical memory size {:#X}",
            pa.as_u64(), sz);
    } else {
        Some(VirtAddr::new(physical_map_addr().as_u64() + pa.as_u64()))
    }
}

pub fn init_memory(
    pmap_va: u64, 
    p_regions: &MemoryRegions,
    kernel_end_va: u64
) {

    let (last_region_index, last_region) = p_regions.iter()
        .enumerate()
        .filter(|(_, r)| r.kind == MemoryRegionKind::Usable)
        .last().unwrap();

    unsafe {
        PHYS_MAP_ADDR = VirtAddr::new(pmap_va);
        PHYS_SIZE = last_region.end;
    }

    let n_frames = get_phys_size() / PAGE_SIZE;
    let sz_refcounts = n_frames * size_of::<PageRefCount>() as u64;
    let heap_start_safe = VirtAddr::new(kernel_end_va + sz_refcounts);
    unsafe {
        HEAP_START = heap_start_safe;
    }

    let mut alloc = unsafe {
        let ptr = kernel_end_va as *mut PageRefCount;
        let page_refcounts = slice::from_raw_parts_mut(ptr, n_frames as usize);
        PageAllocator::new(page_refcounts, p_regions, last_region_index + 1)
    };

    let mut frames = [PhysFrame::containing_address(PhysAddr::new(0)); 100];
    for i in 0..100 {
        frames[i] = alloc.allocate_frame().unwrap();
    }

    for f in frames {
        println!("freeing {:?}", f);
        alloc.free_frame(f);
    }
}

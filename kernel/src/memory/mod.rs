use core::{cmp::max, slice};
use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use heap::HeapAllocator;
use paging::{PageAllocator, PageRefCount};
use x86_64::{structures::paging::{PageTableFlags, Translate}, PhysAddr, VirtAddr};
use crate::scheduling::threads::sync::KIntMutex;

/// Physical page allocation and management
mod paging;
pub use paging::{dealloc_frame, current_pt, SizedPage};

/// Dynamic memory allocation (the heap!)
mod heap;
/// Stack allocation for kernel and user threads
pub mod stacks;
/// Helpers for MMIO-related things
pub mod mmio;
/// Userspace memory management
pub mod user;

/// The beginning of the kernel image (and address spave) in Virtual Memory
pub const KERNEL_START_ADDR: u64 = 0xFFFF_8000_0000_0000;

/// The size of a page in bytes
pub const PAGE_SIZE: u64 = 0x1000;

static mut PHYS_MAP_ADDR: VirtAddr = VirtAddr::new(0);
static mut PHYS_SIZE: u64 = 0;
static mut PHYS_MAP_SIZE: u64 = 0;

static PAGE_ALLOCATOR: KIntMutex<Option<PageAllocator>> = KIntMutex::new(None);
#[global_allocator]
static HEAP_ALLOCATOR: HeapAllocator = HeapAllocator::new();

/// Returns the amount of physical memory on the system
pub fn get_phys_size() -> u64 {
    unsafe {
        PHYS_SIZE
    }
}

/// Returns the size, in bytes, of the heap
pub fn get_heap_size() -> usize {
    HEAP_ALLOCATOR.size()
}

fn physical_map_addr() -> VirtAddr {
    unsafe {
        PHYS_MAP_ADDR
    }
}

/// Resolves a given physical address into the virtual address that maps to it.
/// Used for directly accessing physical memory.
pub fn resolve_phys_addr(pa: PhysAddr) -> Option<VirtAddr> {
    let sz = unsafe { PHYS_MAP_SIZE };
    if pa.as_u64() >= sz {
        panic!(
            "Physical address {:#X} is past the physical memory map size {:#X}",
            pa.as_u64(), sz);
    } else {
        Some(VirtAddr::new(physical_map_addr().as_u64() + pa.as_u64()))
    }
}

/// Resolves some virtual address into the physical address it points to, if
/// mapped
pub fn resolve_virt_addr(va: VirtAddr) -> Option<PhysAddr> {
    current_pt().translate_addr(va)
}

fn find_usable_virtual_space() -> VirtAddr {
    let pt = current_pt();
    for (i, pte) in pt.level_4_table().iter().enumerate() {
        let va = VirtAddr::new_truncate((i as u64) << 39);
        if va.as_u64() < KERNEL_START_ADDR {
            continue;
        }

        // First non-present page we can use!
        if pte.flags() & PageTableFlags::PRESENT == PageTableFlags::empty() {
            return va;
        }
    }

    panic!("Entire kernel virtual address space used!");
}

/// Initializes the memory subsystem by setting up the physical page allocator
/// and the heap allocator
pub fn init_memory(pmap_va: u64, p_regions: &MemoryRegions) {
    let last_usable_region = p_regions.iter()
        .filter(|r| r.kind == MemoryRegionKind::Usable)
        .last().unwrap();

    let last_region = p_regions.iter()
        .last().unwrap();

    // Always cover at least the first 4 GiB of physical memory. That area
    // contains useful MMIO regions (local APIC, I/O APIC, PCI bars) that
    // we want to make accessible to the kernel even if no DRAM exists >4GiB.
    // -- From the bootloader code (it maps at least 4 GiB)
    let pmap_len = max(last_region.end, 0x1_0000_0000);

    unsafe {
        PHYS_MAP_ADDR = VirtAddr::new(pmap_va);
        PHYS_MAP_SIZE = pmap_len;
        PHYS_SIZE = last_usable_region.end;
    }

    let usable_start = find_usable_virtual_space().as_u64();

    let n_frames = get_phys_size() / PAGE_SIZE;
    let sz_refcounts = n_frames * size_of::<PageRefCount>() as u64;
    let heap_start = VirtAddr::new(usable_start + sz_refcounts);

    let pg_alloc = unsafe {
        let ptr = usable_start as *mut PageRefCount;
        let page_refcounts = slice::from_raw_parts_mut(ptr, n_frames as usize);
        PageAllocator::new(page_refcounts, p_regions)
    };

    let mut pg_alloc_lock = PAGE_ALLOCATOR.lock();
    *pg_alloc_lock = Some(pg_alloc);
    drop(pg_alloc_lock);

    unsafe {
        HEAP_ALLOCATOR.init(heap_start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use crate::ktest::*;

    #[repr(align(64))]
    struct Block([u8; 64]);

    ktest! {
        /// The physical map gives a second address for the bytes of the heap.
        fn heap_pointer_resolves_through_the_physical_map() {
            let block = Box::new(Block([0xA5; 64]));
            let va = VirtAddr::from_ptr(&*block);
            let pa = resolve_virt_addr(va).or_fail("resolve_virt_addr")?;
            let alias = resolve_phys_addr(pa).or_fail("resolve_phys_addr")?;
            check_ne!(alias, va);
            let bytes = unsafe { &*(alias.as_ptr::<[u8; 64]>()) };
            check_eq!(bytes, &block.0);
        }

        /// The null page is not mapped.
        fn unmapped_address_resolves_to_none() {
            check!(resolve_virt_addr(VirtAddr::new(0)).is_none());
        }

        /// The last byte of the physical map resolves.
        fn last_byte_of_the_physical_map_resolves() {
            let size = unsafe { PHYS_MAP_SIZE };
            check!(resolve_phys_addr(PhysAddr::new(size - 1)).is_some());
        }

        /// The usable memory ends inside the physical map.
        fn phys_size_is_inside_the_physical_map() {
            check!(get_phys_size() > 0);
            check!(get_phys_size() <= unsafe { PHYS_MAP_SIZE });
        }

        /// An address at the end of the physical map is a kernel error.
        #[should_panic("is past the physical memory map size")]
        fn resolve_phys_addr_past_the_map_panics() {
            let size = unsafe { PHYS_MAP_SIZE };
            let _ = resolve_phys_addr(PhysAddr::new(size));
        }
    }
}

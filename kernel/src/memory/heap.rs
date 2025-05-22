use core::alloc::{GlobalAlloc, Layout};

use spin::Mutex;
use x86_64::{structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, Translate}, VirtAddr};

use crate::memory::{paging::current_pt, PAGE_ALLOCATOR};

struct AllocatorInner {
    heap_start: VirtAddr,
    heap_end: VirtAddr,
    allocations: u64,
    last_alloc: VirtAddr,
}

impl AllocatorInner {
    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let align = layout.align() as u64;
        // This witchcraft is from https://os.phil-opp.com/allocator-designs/#implementing-globalalloc
        let alloc_start = // Aligns up the end of the heap to whats needed
            VirtAddr::new((self.heap_end.as_u64() + align - 1) & !(align - 1));
        let alloc_end = alloc_start + layout.size() as u64;

        let first_page = Page::containing_address(alloc_start);
        let last_page = Page::containing_address(alloc_end - 1);
        let alloc_pages = Page::range_inclusive(first_page, last_page);

        // Make sure all the pages that contain the allocation are mapped, and
        // map them if not
        let mut page_alloc_lock = PAGE_ALLOCATOR.lock();
        let page_alloc = page_alloc_lock.as_mut().unwrap();
        for page in alloc_pages {
            page_alloc.alloc_page(
                page, 
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE
            );
        }

        self.heap_end = alloc_end;
        self.allocations += 1;
        self.last_alloc = alloc_start;
        alloc_start.as_mut_ptr()
    }


    // TODO: Make this better lol
    unsafe fn dealloc(&mut self, ptr: *mut u8, _: Layout) {
        self.allocations -= 1;

        if self.allocations == 0 {
            self.heap_end = self.heap_start;
        } else if self.last_alloc == VirtAddr::from_ptr(ptr) {
            self.heap_end = self.last_alloc;
        }
    }

    fn new(heap_start: VirtAddr) -> AllocatorInner {
        AllocatorInner {
            heap_start,
            heap_end: heap_start,
            allocations: 0,
            last_alloc: heap_start, // Doesnt rlly matter
        }
    }
}

/// The global heap allocator, a bump allocator. Only actually frees memory if
/// the entire heap is free, or if the most recent allocation is freed.
pub struct HeapAllocator {
    inner: Mutex<Option<AllocatorInner>>,
}

impl HeapAllocator {
    /// Creates an uninitialized heap allocator
    pub const fn new() -> HeapAllocator {
        HeapAllocator {
            inner: Mutex::new(None),
        }
    }

    /// Initializes the heap allocator. `unsafe` because the physical page
    /// allocator must be set up by this point, and because the `heap_start`
    /// virtual address must be actually usable for the heap.
    pub unsafe fn init(&self, heap_start: VirtAddr) {
        let mut lock = self.inner.lock();
        assert!(lock.is_none());
        *lock = Some(AllocatorInner::new(heap_start));
    }

    /// The current size of the heap in bytes
    pub fn size(&self) -> usize {
        let lock = self.inner.lock();
        let inner = lock.as_ref().unwrap();
        let size = inner.heap_end.as_u64() - inner.heap_start.as_u64();
        size as usize
    }
}

unsafe impl GlobalAlloc for HeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            self.inner.lock().as_mut().unwrap().alloc(layout)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            self.inner.lock().as_mut().unwrap().dealloc(ptr, layout)
        }
    }
}

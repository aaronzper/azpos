use core::alloc::{GlobalAlloc, Layout};

use spin::Mutex;
use x86_64::{structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, Translate}, VirtAddr};

use crate::memory::{paging::{current_pt, PageSizeType}, PAGE_ALLOCATOR};

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

        // Check if the end of the heap is actually mapped, if not alloc/map it
        let mut pt = current_pt();
        for page in alloc_pages {
            if pt.translate_addr(page.start_address()).is_none() {
                let mut alloc_lock = PAGE_ALLOCATOR.lock();
                let p_alloc = alloc_lock.as_mut().unwrap();

                let p_frame = p_alloc.allocate_frame().expect("Out of memory");
                let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

                let flush = unsafe {
                    pt.map_to(page, p_frame, flags, p_alloc).unwrap()
                };
                flush.flush();
            }
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

pub struct HeapAllocator {
    inner: Mutex<Option<AllocatorInner>>,
}

impl HeapAllocator {
    pub const fn new() -> HeapAllocator {
        HeapAllocator {
            inner: Mutex::new(None),
        }
    }

    pub fn init(&self, heap_start: VirtAddr) {
        let mut lock = self.inner.lock();
        assert!(lock.is_none());
        *lock = Some(AllocatorInner::new(heap_start));
    }

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

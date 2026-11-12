use core::{alloc::{GlobalAlloc, Layout}, ptr::null_mut, sync::atomic::Ordering};
use x86_64::{structures::paging::{Page, PageTableFlags}, VirtAddr};
use crate::{memory::PAGE_ALLOCATOR, scheduling::threads::sync::KIntMutex};
use super::stacks::STACKS_BOTTOM;

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

        // If this allocation reaches into kernel stacks
        let stack_bottom = VirtAddr::new(STACKS_BOTTOM.load(Ordering::SeqCst));
        // Ignore if bottom is zero cause that means thers no stack
        if stack_bottom != VirtAddr::zero() && alloc_end > stack_bottom {
            println!("Heap allocation ran into stack!");
            return null_mut();
        }

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
    inner: KIntMutex<Option<AllocatorInner>>,
}

impl HeapAllocator {
    /// Creates an uninitialized heap allocator
    pub const fn new() -> HeapAllocator {
        HeapAllocator {
            inner: KIntMutex::new(None),
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{alloc::{alloc, dealloc}, vec::Vec};
    use crate::{ktest::*, memory::{get_heap_size, HEAP_ALLOCATOR, PAGE_SIZE}};

    fn heap_end() -> u64 {
        let lock = HEAP_ALLOCATOR.inner.lock();
        lock.as_ref().map_or(0, |inner| inner.heap_end.as_u64())
    }

    ktest! {
        /// Each allocation has the alignment of its layout.
        fn allocation_has_the_requested_alignment() {
            for align in [1, 8, 64, 4096] {
                let layout = Layout::from_size_align(1, align).or_fail("layout")?;
                let ptr = unsafe { alloc(layout) };
                let addr = ptr as usize;
                if !ptr.is_null() {
                    unsafe { dealloc(ptr, layout) };
                }
                check!(!ptr.is_null(), "align {}", align);
                check!(addr % align == 0, "align {}, address {:#x}", align, addr);
            }
        }

        /// A 1 MiB allocation is writable at its first and last byte.
        fn large_allocation_is_writable_at_both_ends() {
            let layout = Layout::from_size_align(1 << 20, 1).or_fail("layout")?;
            let ptr = unsafe { alloc(layout) };
            check!(!ptr.is_null());
            let (first, last) = unsafe {
                ptr.write_volatile(0x5A);
                ptr.add((1 << 20) - 1).write_volatile(0xA5);
                (ptr.read_volatile(), ptr.add((1 << 20) - 1).read_volatile())
            };
            unsafe { dealloc(ptr, layout) };
            check_eq!(first, 0x5A);
            check_eq!(last, 0xA5);
        }

        /// A free of the newest allocation gives its bytes back to the heap.
        fn free_of_the_newest_allocation_shrinks_the_heap() {
            let layout = Layout::from_size_align(64, 1).or_fail("layout")?;
            let before = get_heap_size();
            let ptr = unsafe { alloc(layout) };
            let during = get_heap_size();
            unsafe { dealloc(ptr, layout) };
            let after = get_heap_size();
            check!(during >= before + 64, "before {} during {}", before, during);
            check_eq!(after, before);
        }

        /// An allocation that reaches the kernel stacks gives null.
        fn allocation_into_the_stacks_gives_null() {
            let bottom = STACKS_BOTTOM.load(Ordering::SeqCst);
            let size = (bottom - heap_end() + PAGE_SIZE) as usize;
            let layout = Layout::from_size_align(size, 1).or_fail("layout")?;
            let ptr = unsafe { alloc(layout) };
            if !ptr.is_null() {
                unsafe { dealloc(ptr, layout) };
            }
            check!(ptr.is_null());
        }

        /// An allocation larger than the address space gives null.
        /// Today the address arithmetic panics before the stack check.
        #[known_bug(31, crash)]
        fn allocation_larger_than_the_address_space_gives_null() {
            let layout = Layout::from_size_align(1 << 47, 1).or_fail("layout")?;
            let ptr = unsafe { alloc(layout) };
            if !ptr.is_null() {
                unsafe { dealloc(ptr, layout) };
            }
            check!(ptr.is_null());
        }

        /// The heap uses freed memory again.
        /// Today the bump allocator frees only the newest allocation.
        #[known_bug(31)]
        fn freed_memory_is_used_again() {
            let mut kept: Vec<*mut u8> = Vec::with_capacity(256);
            let big = Layout::from_size_align(4096, 1).or_fail("layout")?;
            let small = Layout::from_size_align(1, 1).or_fail("layout")?;
            let before = get_heap_size();

            for _ in 0..256 {
                let a = unsafe { alloc(big) };
                let b = unsafe { alloc(small) };
                unsafe { dealloc(a, big) };
                kept.push(b);
            }
            let growth = get_heap_size() - before;

            for b in kept.drain(..).rev() {
                unsafe { dealloc(b, small) };
            }
            check!(growth < 256 * 1024, "the heap grew by {} bytes", growth);
        }
    }
}

use core::sync::atomic::{AtomicU64, Ordering};
use bitvec::vec::BitVec;
use lazy_static::lazy_static;
use x86_64::{structures::paging::{mapper::CleanUp, FrameDeallocator, Mapper, Page, PageTableFlags}, VirtAddr};
use crate::scheduling::threads::sync::KIntMutex;

use super::{paging::{current_pt, SizedPage}, PAGE_ALLOCATOR, PAGE_SIZE};

// Will wrap around to 0xFFFF_FFFF_FFFF_F000 as the root
const KERNEL_STACK_ROOT: VirtAddr = VirtAddr::new(0u64.wrapping_sub(PAGE_SIZE)); 
const KERNEL_STACK_SIZE: usize = 4;

/// Stores the lowest address in use by the stack allocator
///
/// Stored as a seperate static to prevent deadlocking
pub static STACKS_BOTTOM: AtomicU64 = 
    AtomicU64::new(KERNEL_STACK_ROOT.as_u64() - 1);

lazy_static! {
    pub static ref KERNEL_STACK_ALLOCATOR: KIntMutex<StackAllocator> =
        KIntMutex::new(StackAllocator::new(KERNEL_STACK_ROOT, KERNEL_STACK_SIZE));
}

/// Allocates kernel stacks.
pub struct StackAllocator {
    /// The address to start from. The top of the first stack will be here
    root: VirtAddr,
    /// The size of each stack in pages, excluding the guard page
    stack_size: usize,
    /// Tracks whether the stack at index n exists or not. Stack 0 starts at
    /// the root, stack 1 starts at `root - (PAGE_SIZE * (stack_size + 1))`, etc
    stacks: BitVec,
    /// Next index to allocate
    next_index: usize,
}

impl StackAllocator {
    /// Creates a new `StackAllocator`
    pub fn new(root: VirtAddr, stack_size: usize) -> StackAllocator {
        StackAllocator {
            root,
            stack_size,
            stacks: BitVec::new(),
            next_index: 0,
        }
    }

    /// Returns the total number of bytes used by a single stack, including the
    /// guard page
    fn total_bytes_per_stack(&self) -> usize {
        (self.stack_size + 1) * PAGE_SIZE as usize
    }

    fn top_addr(&self, index: usize) -> VirtAddr {
        self.root - (index * self.total_bytes_per_stack()) as u64
    }

    /// Allocates a new `KThreadStack`. Returns `None` if there's no space left
    /// (detected if the allocator hits mapped virtual memory, indicating the
    /// heap got up there).
    pub fn alloc_stack(&mut self) -> Option<KThreadStack> {
        let optional_index = self.stacks.iter()
            .by_vals()
            .enumerate()
            .skip(self.next_index)
            .find(|(_, alloced)| !alloced);

        let index = match optional_index {
            Some((i, _)) => i,
            None => self.stacks.len(), // If we dont find any usable stacks, 
                                       // add a new one at the end
        };

        let top_addr = self.top_addr(index);
        let guard_page_addr = top_addr - self.total_bytes_per_stack() as u64;
        let bottom_addr = guard_page_addr + PAGE_SIZE;
        
        if index == self.stacks.len() {
            self.stacks.push(true);
        } else if index < self.stacks.len() {
            self.stacks.set(index, true);
        } else {
            panic!("Index great than stacks length... somehow");
        }

        self.next_index = index + 1;
        STACKS_BOTTOM.fetch_min(guard_page_addr.as_u64(), Ordering::AcqRel);

        let guard_page = SizedPage::from_start_address(guard_page_addr).unwrap();
        let top_page = SizedPage::from_start_address(top_addr).unwrap();
        let bottom_page = SizedPage::from_start_address(bottom_addr).unwrap();

        let pt = current_pt();

        let all_pages = Page::range(guard_page, top_page);
        for page in all_pages {
            if pt.translate_page(page).is_ok() {
                println!("poopy -zelda");
                return None; // We hit the heap
            }
        }

        let mut page_alloc_lock = PAGE_ALLOCATOR.lock();
        let page_alloc = page_alloc_lock.as_mut().unwrap();
        let usable_pages = Page::range(bottom_page, top_page);
        for page in usable_pages { 
            page_alloc.alloc_page(
                page, 
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE
            );
        }

        Some(KThreadStack { top: top_addr, index, })
    }

    fn free_stack(&mut self, index: usize) {
        if !self.stacks[index] {
            panic!("Stack double free!");
        }

        let top_addr = self.top_addr(index);
        let bottom_addr = 
            top_addr - self.total_bytes_per_stack() as u64 + PAGE_SIZE;
        let top_page = SizedPage::from_start_address(top_addr).unwrap();
        let bottom_page = SizedPage::from_start_address(bottom_addr).unwrap();

        let mut page_alloc_lock = PAGE_ALLOCATOR.lock();
        let page_alloc = page_alloc_lock.as_mut().unwrap();
        let mut pt = current_pt();

        let pages = Page::range_inclusive(bottom_page, top_page - 1);

        unsafe {
            for page in pages {
                let (frame, flush) = pt.unmap(page)
                    .expect("Failed to unmap stack page");
                page_alloc.deallocate_frame(frame);
                flush.flush();
            }
            pt.clean_up_addr_range(pages, page_alloc);
        }

        self.stacks.set(index, false);

        if index < self.next_index {
            self.next_index = index;
        }
    }
}

/// An individual stack on a kthread
pub struct KThreadStack {
    /// The top of the stack
    top: VirtAddr,
    /// The index of the stack (for the allocator)
    index: usize,
}

impl KThreadStack {
    /// Returns top of the stack
    pub fn top(&self) -> VirtAddr {
        self.top
    }
}

impl Drop for KThreadStack {
    fn drop(&mut self) {
        KERNEL_STACK_ALLOCATOR.lock().free_stack(self.index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::hint::black_box;
    use crate::{ktest::*, memory::resolve_virt_addr};

    /// The number of bytes of one stack, with the guard page
    const STACK_BYTES: u64 = (KERNEL_STACK_SIZE as u64 + 1) * PAGE_SIZE;

    fn new_stack() -> Option<KThreadStack> {
        KERNEL_STACK_ALLOCATOR.lock().alloc_stack()
    }

    fn stacks_in_use() -> usize {
        KERNEL_STACK_ALLOCATOR.lock().stacks.count_ones()
    }

    /// Calls itself with a 1 KiB local array until the stack overflows.
    fn recurse(depth: u64) -> u64 {
        let mut local = [0u8; 1024];
        local[0] = depth as u8;
        black_box(&mut local);
        recurse(depth + 1) + local[0] as u64
    }

    ktest! {
        /// Each page of a new stack is mapped and writable.
        fn new_stack_is_mapped_and_writable() {
            let stack = new_stack().or_fail("alloc_stack")?;
            let top = stack.top().as_u64();
            check_eq!(top % PAGE_SIZE, 0);
            for i in 1..=KERNEL_STACK_SIZE as u64 {
                let addr = top - i * PAGE_SIZE;
                check!(resolve_virt_addr(VirtAddr::new(addr)).is_some(), "{:#x}", addr);
                let ptr = addr as *mut u64;
                let value = unsafe {
                    ptr.write_volatile(0x1234_5678);
                    ptr.read_volatile()
                };
                check_eq!(value, 0x1234_5678);
            }
        }

        /// The page below the stack is a guard page with no mapping.
        fn guard_page_below_the_stack_is_not_mapped() {
            let stack = new_stack().or_fail("alloc_stack")?;
            let guard = stack.top().as_u64() - STACK_BYTES;
            check!(resolve_virt_addr(VirtAddr::new(guard)).is_none());
            check!(STACKS_BOTTOM.load(Ordering::SeqCst) <= guard);
        }

        /// Two stacks do not overlap.
        fn two_stacks_do_not_overlap() {
            let a = new_stack().or_fail("first alloc_stack")?;
            let b = new_stack().or_fail("second alloc_stack")?;
            let (a_top, b_top) = (a.top().as_u64(), b.top().as_u64());
            check!(a_top <= b_top - STACK_BYTES || b_top <= a_top - STACK_BYTES,
                "tops {:#x} and {:#x}", a_top, b_top);
        }

        /// A dropped stack frees its index. The next stack gets the same top.
        fn dropped_stack_is_used_again() {
            let first = new_stack().or_fail("first alloc_stack")?;
            let top = first.top();
            drop(first);
            let second = new_stack().or_fail("second alloc_stack")?;
            check_eq!(second.top(), top);
        }

        /// A thread that returns frees its kernel stack.
        fn ended_threads_free_their_stacks() {
            let before = stacks_in_use();
            for _ in 0..8 {
                run_in_thread(|| {});
            }
            check!(wait_until(|| stacks_in_use() <= before),
                "{} stacks before, {} after", before, stacks_in_use());
        }

        /// A second free of one stack is a kernel error.
        #[should_panic("Stack double free!")]
        fn double_free_of_a_stack_panics() {
            let stack = new_stack().or_fail("alloc_stack")?;
            let index = stack.index;
            drop(stack);
            KERNEL_STACK_ALLOCATOR.lock().free_stack(index);
        }

        /// A stack overflow hits the guard page and gives a double fault.
        /// The page fault has no handler, so the CPU escalates it.
        #[should_panic("Double Fault")]
        fn stack_overflow_gives_a_double_fault() {
            run_in_thread(|| { black_box(recurse(0)); });
        }
    }
}

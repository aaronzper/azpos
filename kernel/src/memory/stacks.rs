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

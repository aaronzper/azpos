# Stage 3 — Memory Management: Real Heap, User Page Lifecycle, page_alloc/page_free

## Goal

Replace the never-freeing bump allocators (kernel and userspace) with a real
free-list allocator, implement teardown of a process's user memory (required by
`exit` in Stage 4 and CoW in Stage 12), and add the `page_alloc`/`page_free`
syscalls from the design doc.

## Prerequisites

Stage 2 (usermem validation, error codes).

## Current State

- Kernel heap: `kernel/src/memory/heap.rs` — bump allocator growing upward from
  `heap_start`; only frees if the *last* allocation is freed or everything is freed.
  Long-running daemons will exhaust memory.
- Userspace heap: `libraries/libsystem/src/lib.rs` uses `wee_alloc`
  (unmaintained/archived; no growth mechanism).
- Frame allocator: `kernel/src/memory/paging.rs` — `PageAllocator` with a
  `PageRefCount` array covering all physical frames (refcounts already exist —
  good for CoW later), `alloc_page(page, flags)`, `dealloc_frame`. Kernel stacks
  already free correctly (`stacks.rs::free_stack` unmaps + deallocates + cleans up
  page tables via `clean_up_addr_range`).
- User pages: `memory/user.rs::alloc_user_pages` maps pages into the *current* lower
  half. Nothing ever unmaps user memory; `Process` only stores L4 entries
  (`user_page_table: [PageTableEntry; 256]`).
- `USER_END_ADDR = 0x0000_8000_0000_0000`; user stack = 4 pages just below
  `USER_END_ADDR - PAGE_SIZE` (`processes/mod.rs:71`).

## Design Decisions

1. **One allocator implementation, two users.** New `#[no_std]` crate
   `libraries/freelist-alloc`:
   - First-fit free list with block headers `{ size: usize, next: *mut Header }`,
     16-byte minimum block, split on allocate, coalesce with both neighbors on free
     (list kept address-sorted to make coalescing O(1) at insert).
   - Generic over a backend: `trait PageSource { fn grow(&mut self, min_bytes: usize) -> Option<(*mut u8, usize)>; }`
     — returns a newly mapped, contiguous virtual region appended to the managed area.
   - No shrinking/unmapping (roadmap).
   - Unit-testable on the host: this crate is plain Rust — add `#[cfg(test)]` tests
     (alloc/free/coalesce/alignment patterns) runnable with
     `cargo test -p freelist-alloc` on the host target. This is the one place we get
     cheap host-side tests; use them.
2. **Kernel backend:** keep the current heap region plan (grows upward from
   `heap_start`, must not collide with `STACKS_BOTTOM` — keep that check). Replace
   `AllocatorInner` in `heap.rs` with the freelist allocator behind the existing
   `KIntMutex` + `#[global_allocator]` facade. Keep `HeapAllocator::size()` (now
   "bytes currently mapped") and add `used()` for Stage 4's `sysinfo`.
3. **Userspace backend:** `libsystem` drops `wee_alloc`; global allocator =
   freelist-alloc with a `PageSource` that calls the new `PageAlloc` syscall to map
   pages in a dedicated user heap region. **User address-space layout convention**
   (document in `libsci` rustdoc):
   - ELF segments: wherever the linker put them (low addresses)
   - User heap region: grows up from `0x0000_1000_0000_0000`
   - Thread stacks: grow down from `USER_END_ADDR` (Stage 12 adds more stacks)
4. **`PageAlloc(addr, n_pages, flags)` / `PageFree(addr, n_pages)` ABI:**
   - `addr` page-aligned, `addr + n_pages*4096 <= USER_END_ADDR`, else `BadAddress`.
   - `flags`: bit0 = writable, bit1 = executable (read is implied).
   - `PageAlloc` fails with `AlreadyExists` if *any* page in the range is already
     mapped; on success all pages are mapped zeroed (`USER_ACCESSIBLE` always set).
     Zeroing matters: frames are recycled and must not leak kernel/other-process data.
   - `PageFree` fails with `BadAddress` if any page in the range is unmapped or not
     `USER_ACCESSIBLE`; on success unmaps + decrements refcounts.
   - Return 0 on success.
5. **User-half teardown:** `pub fn free_user_half(proc: &Process)` in
   `memory/user.rs`. Walks the process's saved `user_page_table` L4 entries (use
   `resolve_phys_addr` to reach L3/L2/L1 tables through the physical map — do *not*
   require the process's PT to be loaded), deallocates every mapped 4 KiB frame
   (refcount-aware) and every intermediate page-table frame, and zeroes the L4
   entries. Used by `exit`/`kill` (Stage 4). Must also handle being called for the
   *currently loaded* process (caller switches/clears the live lower half first —
   Stage 4 wires this).
6. The ELF loader thread and stack setup in `processes/mod.rs` stay as-is; they
   already go through `alloc_user_pages`.

## Work Items

1. Create `libraries/freelist-alloc` with host unit tests.
2. Swap kernel heap internals; boot-time smoke check: in `kmain` (always-on, cheap),
   allocate/free 10k mixed-size `Box`es and assert the heap mapped size stays
   bounded (< 1 MiB), then `println!("[KSELFTEST] heap PASS")`. The test runner
   treats a `[KSELFTEST] <name> FAIL` line as failure (extend the Stage 1 runner
   verdict logic).
3. Implement `PageAlloc`/`PageFree` (syscalls 9/10): libsci variants + docs, kernel
   handlers (validation per design), `libsystem` wrappers
   `sys_page_alloc/sys_page_free`.
4. Implement `free_user_half` + plumb `dealloc` paths in `PageAllocator` as needed
   (it already has refcounts and `dealloc_frame`; add an `unmap_user_page` helper
   that unmaps from a *given* process's table).
5. Userspace allocator: implement `PageSource` over `PageAlloc` in `libsystem`,
   remove `wee_alloc`, set the new `#[global_allocator]`.
6. ktest additions:
   - `page_alloc_rw`: alloc 4 pages at `0x2000_0000_0000`, write/read a pattern
     across all of them, free, realloc same address succeeds and reads back **zeroes**.
   - `page_alloc_overlap_rejected`: allocating over the program image → `-11`;
     over kernel space → `-4`; unaligned → `-4`.
   - `page_free_invalid`: freeing an unmapped range → `-4`; double free → `-4`.
   - `heap_stress`: userspace `Vec` growth to ~1 MiB then drop, repeated 10×
     (exercises the new user allocator + grow path; passes if it completes).

## Deliverables

- `freelist-alloc` crate (host-tested) used by both kernel heap and libsystem.
- `PageAlloc`/`PageFree` syscalls with zeroed-page guarantee.
- `free_user_half` ready for process teardown.
- `wee_alloc` dependency removed.

## Completion Criteria

1. `cargo test -p freelist-alloc` passes on the host.
2. `cargo run -- -test` passes all prior + new tests; serial shows
   `[KSELFTEST] heap PASS`.
3. `heap_stress` proves userspace allocations beyond the old static-arena capacity
   work (i.e. growth via `PageAlloc` actually happens — verify by making the initial
   user heap grant small, e.g. 16 pages per `grow` call).
4. Normal `cargo run` boots and runs adam unchanged (manual).

## Out of Scope

- Freeing/unmapping from the allocator back to the page allocator (roadmap:
  "better allocator").
- Guard pages for user stacks, demand paging, lazy allocation.
- CoW machinery (Stage 12) — but note `PageRefCount` is already shared-aware.

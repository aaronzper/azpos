# Stage 12 — Multi-threading per Process & fork with Copy-on-Write — STAGE II COMPLETE

## Goal

The last two Stage II kernel features: processes with multiple user threads
(`ThreadSpawn`/`ThreadExit`/`ThreadJoin`) and `fork` with copy-on-write memory.
This is the hardest stage; both features reach deep into scheduling, memory, and
the resource model. Do threads first (smaller, independently shippable), then fork.

## Prerequisites

All prior stages (notably 3: refcounted frames + teardown; 4: reaper; 7: clone
semantics).

## Current State

- One thread per process, created only inside `spawn_proc`. `Thread` couples a
  kernel entrypoint trampoline (`run_thread`) with a kstack;
  user threads enter ring 3 via a hand-built `iretq` frame.
- `terminate_proc` (Stage 4) already loops over "all threads tagged with pid".
- `PageRefCount` array exists (Stage 3 uses it for shared-frame dealloc) — CoW was
  anticipated.
- **No page-fault handler exists** (only breakpoint/double-fault/timer/keyboard/
  mouse). CoW requires one; it also massively improves crash diagnostics.
- Syscall entry reads the per-thread kstack from `gs:[0]`
  (`CURRENT_USER_KSTACK_PTR`), set per-thread by the scheduler. The user's
  RIP/RFLAGS/RSP at syscall time are pushed on the kstack by `entrypoint.s`
  (layout: `[top-8]=user rsp, [top-16]=user rip (rcx), [top-24]=user rflags (r11)`)
  — fork needs these to build the child's resume state.

## Part A — User Threads

### Design

1. **User stack allocation per process:** `Process` gains a stack-slot allocator:
   slot 0 = the existing main stack (4 pages + guard gap below
   `USER_END_ADDR - PAGE_SIZE`); slot N tops at
   `USER_END_ADDR - PAGE_SIZE - N * 0x40_0000` (4 MiB stride, 16 KiB mapped, rest
   is guard/headroom). Track a free-slot bitmask; slots are freed on thread exit.
2. **Syscalls (36–38):**
   - `ThreadSpawn(entry_ptr, arg) -> tid`: validates `entry_ptr` is user-mapped
     +executable; allocates a stack slot + kstack; creates a thread whose initial
     `CpuState` is *directly* a ring-3 state (new constructor
     `Thread::new_user_thread(pid, entry, rsp, arg)` — user segments from
     `GDT.user_code/user_data`, `rdi = arg`, no kernel trampoline). Returns a
     process-scoped… **decision: thread IDs are the global scheduler `ThreadID`**
     (simpler; they're opaque to userspace anyway).
   - `ThreadExit() -> !`: kills only the calling thread (reaper path); frees its
     stack slot; if it was the **last** thread of the process → full
     `terminate_proc` (a process with zero threads is dead).
   - `ThreadJoin(tid) -> 0`: blocks until `tid` (must belong to the caller's
     process, else `-3`) no longer exists. Implementation: scheduler keeps
     `joiners: Vec<(waited_tid, BlockedThread)>`, drained when threads are reaped.
     Joining a finished/unknown tid returns 0 immediately (idempotent).
   - Process `Exit`/`Kill` terminate **all** threads (already a loop — now it does
     real work; make sure killing N threads where one is the caller works: kill
     others first, self last).
3. **Userspace API** (`libsystem::thread`): `spawn(fn(usize), arg) -> Tid`,
   `join(Tid)`, `exit()`. Closures: out of scope (raw fn + usize arg; a Box-leaking
   closure wrapper is a nice-to-have if trivial). Add `Mutex`/`SpinLock` (atomic +
   `Yield` loop) to libsystem for test use.
4. **Audit pass (do this deliberately):** the kernel was written single-thread-per-
   process. Check at minimum: `CURRENT_USER_KSTACK_PTR` is per-*thread* via the
   scheduler (OK by design — verify), `Process.user_page_table` save/load when two
   runnable threads share one process (page-table save/load between same-process
   switches must be a no-op or at least correct — currently `save_page_tables`/
   `load_page_tables` run on every switch; correct but wasteful: skip when
   old_pid == new_pid), resource table mutation from two threads (all access is
   under `PROCESSES` KIntMutex — OK), and **blocking syscalls from two threads of
   one process concurrently** (each has its own kstack — OK).

### Tests (ktest)

- `thread_basic`: spawn 4 threads incrementing a shared `AtomicU64` 10k times
  each through a libsystem Mutex-guarded counter too; join all; both counters
  exact.
- `thread_join_finished`: join after the thread already exited → returns.
- `thread_last_exit`: child proc whose main thread `ThreadExit`s while a spawned
  thread keeps running 200 ms then exits — process stays alive until the last
  thread dies (`ProcInfo` n_threads observable), then fully reclaims.
- `thread_kill_multi`: `Kill` a child with 3 live threads; everything reclaims
  (heap-stability loop ×10).
- `thread_stack_isolation`: two threads take large stack frames (recursion depth)
  without corrupting each other; guard gap means a runaway recursion **page-faults
  cleanly** (after Part B's #PF handler: faults in user → kill process, don't
  panic the kernel — test: child recurses forever, parent observes child death,
  kernel survives).

## Part B — fork + Copy-on-Write

### Design

1. **Page-fault handler first** (`interrupts/handlers.rs` + IDT entry, with error
   code): decode CR2 + error code. Cases:
   - Write fault, user-mode, on a present page marked CoW → CoW resolution (below).
   - Any other user-mode fault → `println!` diagnostics, terminate the faulting
     process (not the kernel), schedule away. (This alone is a big robustness win:
     userspace bugs stop double-faulting the machine.)
   - Kernel-mode fault → panic with CR2/RIP (as informative as possible).
2. **CoW marking:** page-table bit 9 (`PageTableFlags::BIT_9`, available-to-OS) =
   "CoW page". `fork` walks the parent's user half: for each present 4 KiB leaf:
   shared-map it into the child (same frame, refcount++ via `PageRefCount`); if it
   was writable: clear WRITABLE, set BIT_9 in **both** parent and child entries;
   read-only pages stay shared forever. Intermediate tables are fresh allocations
   for the child. TLB-flush the parent's lower half after marking.
3. **CoW resolution** (in #PF): refcount of the frame == 1 → just set WRITABLE,
   clear BIT_9 (last owner). Else: allocate a new frame, copy 4 KiB (via the
   physical map), map it writable, refcount-- on the old frame. All under the page
   allocator lock; single-CPU means no racing faults.
4. **`Fork() -> pid (parent) / 0 (child)`** (syscall 39, named `ThreadFork` in the
   enum table — rename freely, keep `= 39`):
   - Only the calling thread is duplicated (POSIX semantics; other threads don't
     exist in the child). Multi-threaded callers: allowed, documented hazard
     (locks held by absent threads stay locked in the child — userspace's
     problem, same as POSIX).
   - New `Process` with CoW'd memory (above) and a resource table built by
     `try_clone` on every entry **in the same RIDs** (IDTable needs
     insert-with-id); non-cloneable resources (devices, files for now) are *omitted*
     from the child — documented (POSIX would share; we diverge honestly rather
     than fake it; revisit with file `try_clone` on the roadmap).
   - Child thread construction: fresh kstack; its `CpuState` must resume in
     userspace exactly at the syscall return point with `rax = 0`. Build it from
     the parent's syscall frame: user RIP/RSP/RFLAGS are on the parent's kstack at
     the fixed `entrypoint.s` offsets (export those as named consts from
     `syscalls/mod.rs` and `static_assert`-style check them in `entrypoint.s`
     comments). Callee-saved user registers were already saved on the **user**
     stack by `make_syscall.s` (which the child shares via CoW), so resuming at
     the user return address with only RIP/RSP/RFLAGS/rax set is ABI-correct.
   - `user: uid` inherits; name = parent's name + `*`.
5. **`free_user_half` + CoW interplay:** teardown decrements refcounts (already
   refcount-aware from Stage 3); a CoW frame with remaining refs is *not* freed —
   verify Stage 3 implemented exactly this; the surviving sibling keeps a
   read-only CoW page whose refcount may now be 1 (lazily fixed by its next write
   fault — fine).

### Tests (ktest)

- `fork_basic`: fork; child writes "child" to a pre-made pipe and exits;
  parent reads it, `rax` values correct on both sides.
- `fork_cow_isolation`: parent fills a 256 KiB buffer with pattern A; fork; child
  rewrites it with pattern B, signals via pipe, exits; parent verifies pattern A
  intact (CoW actually copied).
- `fork_cow_sharing`: fork 8 children that only *read* a large buffer then exit;
  `Sysinfo` physical/heap usage stays near-flat (shared pages never copied) —
  assert usage delta < buffer_size.
- `fork_exec_pattern`: fork; child uses `Spawn`… (no exec syscall — instead child
  does real work: opens a fresh pipe set and spawns a program; proves forked
  children are fully functional processes).
- `pagefault_kills_not_panics`: child dereferences null → parent observes child
  gone; suite continues (also covered in Part A test, keep both).
- Soak: `fork_stress` — fork/exit ×100 with CoW writes; heap + n_procs stable.

## Deliverables

- Syscalls 36–39, libsystem `thread` module + `fork()` wrapper.
- Page-fault handler with user-process termination semantics.
- CoW machinery on the refcounted frame allocator.

## Completion Criteria

1. `cargo run -- -test` passes the full suite (every stage's tests still green —
   especially Stage 4 sleep/reaper and Stage 7 leak checks, which CoW/threads
   could regress).
2. `fork_cow_sharing`'s memory assertion proves pages are actually shared, and
   `fork_cow_isolation` proves they're actually copied.
3. `pagefault_kills_not_panics`: a userspace segfault no longer brings down the
   kernel.
4. Check off the final stages in `00-OVERVIEW.md`; tag `stage-2-complete` (ask
   the user before tagging/pushing). The design doc's Stage I + Stage II feature
   matrix is now fully implemented.

## Out of Scope

- exec-style replace-image syscall, signals, thread-local storage, futexes,
  SMP (all roadmap).
- Sharing non-cloneable resources across fork (roadmap, with file `try_clone`).

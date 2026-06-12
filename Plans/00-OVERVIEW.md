# azpOS Implementation Plans — Overview

This directory contains staged implementation plans that take azpOS from its current
state (end of "Stage I core": multitasking, VM, ELF loading, basic syscalls) to the
full design in the azpOS reference doc: a complete Stage I userspace (console, shell,
file/device/process syscalls) and all of Stage II (IPC, System Registry, users,
multi-threading, fork).

**How to use these plans (read this first, future Claude):**

1. Stages are ordered by dependency. Do them in order unless a stage's
   "Prerequisites" section says otherwise.
2. One stage ≈ one work session / one PR. Each plan has explicit deliverables and
   machine-checkable completion criteria.
3. Before starting a stage: read `CLAUDE.md`, read the stage plan top to bottom, and
   run `cargo run -- -test` (after Stage 1 exists) to confirm a green baseline.
4. After finishing a stage: all completion criteria pass, then check off the stage
   in the table below (edit this file). This table is the single source of truth for
   project status; `CLAUDE.md` stays minimal and static.
5. The designs in these plans are decisions, not suggestions. Where a plan says
   "Design decision", implement it as written unless it is *impossible*; if you must
   deviate, document the deviation in the plan file under a `## Deviations` heading.

## Stage Table

| Stage | Plan | Theme | Status |
|-------|------|-------|--------|
| 1 | `01-test-harness.md` | Automated QEMU test harness | ☐ |
| 2 | `02-syscall-hardening.md` | Safe syscall dispatch, user-pointer validation, `seek`/`get_type`/`get_info` | ☐ |
| 3 | `03-memory.md` | Free-list kernel heap, user page teardown, `page_alloc`/`page_free` | ☐ |
| 4 | `04-process-lifecycle.md` | Time, `sleep`, `exit`, `kill`, `getpid`, `pinfo`, klog, `sysinfo`, `power` | ☐ |
| 5 | `05-vfs.md` | VFS, FAT32 write support, file syscalls | ☐ |
| 6 | `06-devices.md` | `opendev`, keyboard/framebuffer/mouse device resources | ☐ |
| 7 | `07-spawn-and-pipes.md` | `spawn` with args + handle passing, pipes | ☐ |
| 8 | `08-stage1-userspace.md` | `console`, `azpsh`, `info`, `syslogd` — **Stage I complete** | ☐ |
| 9 | `09-ipc.md` | Named-port IPC (`ipc_listen`/`ipc_bind`/`ipc_connect`) | ☐ |
| 10 | `10-registry.md` | `registryd` + System Registry + client API | ☐ |
| 11 | `11-users.md` | Users/permissions, login, multi-TTY, FB sharing | ☐ |
| 12 | `12-threads-and-fork.md` | Multi-threading per process, `fork` + CoW — **Stage II complete** | ☐ |
| — | `99-roadmap.md` | Post-Stage-II roadmap (ext2, networking, USB, Doom) | n/a |

## Global Constraints (apply to every stage)

- **Toolchain:** nightly Rust, `#[no_std]` kernel and userspace, artifact-dependency
  build. Never introduce `std` into `kernel/`, `libraries/`, or `programs/`.
- **Boot targets:** BIOS is the primary, tested target. UEFI must continue to *boot to
  the kernel banner* (don't break `cargo run -- -uefi`), but UEFI doesn't get the data
  partition today and userspace tests only run on BIOS (see Stage 1).
- **ABI stability:** the `Syscall` enum in `libsci` is append-only with **explicit
  discriminants** (see table below). Serialized `libsci` types use `postcard`; enum
  variants in serialized types are also append-only (postcard encodes variant index).
- **Locking rules:** `KIntMutex` (interrupt-disabling spinlock) for anything touched
  from interrupt handlers; `KMutex` (blocking) otherwise. Lock ordering when nesting:
  `SCHEDULER` → `PROCESSES` → everything else. Prefer grabbing the PID and dropping
  the scheduler lock before locking `PROCESSES` (see `with_current_proc` helper,
  Stage 2).
- **Interrupt handlers run on a shared IST stack** (`PIC_STACK` in
  `kernel/src/interrupts/mod.rs`), not on the interrupted thread's kstack. Handlers
  must **never block, take a `KMutex`, or call `thread_yield`** — `KIntMutex` only.
  Re-entering the IST stack (e.g. by yielding from an IRQ handler) would clobber the
  live interrupt frame.
- **Blocking in syscalls is allowed**: every thread has its own kernel stack
  (`gs:[0]`), so a syscall handler may block on `KMutex`/`KCondvar` and the scheduler
  will switch away. Never block while holding a `KIntMutex`.
- **Docs:** rustdoc on all new public items, matching the existing style. Syscall
  semantics are documented on the `Syscall` enum variants in `libsci` — that is the
  ABI source of truth.
- **Tests:** every stage adds cases to the `ktest` userspace test program (Stage 1)
  exercising its new syscalls/behaviors. A stage is not done until
  `cargo run -- -test` passes.

## Syscall Number Allocation (authoritative)

Existing (implicit discriminants 0–6, do not reorder):
`Yield=0, GetLogger=1, Read=2, Write=3, Close=4, Seek=5, ListDevices=6`

Planned (add with explicit `= N` in the stage that implements them):

| N | Syscall | Stage | | N | Syscall | Stage |
|---|---------|-------|-|---|---------|-------|
| 7 | GetType | 2 | | 24 | Delete | 5 |
| 8 | GetInfo | 2 | | 25 | LockDir | 5 |
| 9 | PageAlloc | 3 | | 26 | UnlockDir | 5 |
| 10 | PageFree | 3 | | 27 | OpenDevice | 6 |
| 11 | Time | 4 | | 28 | SetMode | 6 |
| 12 | Sleep | 4 | | 29 | Spawn | 7 |
| 13 | Exit | 4 | | 30 | Pipe | 7 |
| 14 | Kill | 4 | | 31 | IpcListen | 9 |
| 15 | GetPid | 4 | | 32 | IpcBind | 9 |
| 16 | ProcInfo | 4 | | 33 | IpcConnect | 9 |
| 17 | Klog | 4 | | 34 | SetUser | 11 |
| 18 | Sysinfo | 4 | | 35 | SetOwner | 11 |
| 19 | Power | 4 | | 36 | ThreadSpawn | 12 |
| 20 | OpenFile | 5 | | 37 | ThreadExit | 12 |
| 21 | ListDir | 5 | | 38 | ThreadJoin | 12 |
| 22 | MakeDir | 5 | | 39 | ThreadFork (fork) | 12 |
| 23 | Move | 5 | | | | |

## Error Code Allocation (authoritative, defined in Stage 2)

Negative `rax` values. `-1..-0xFFFF` reserved for standard codes; `<= -0x10000` is
`Misc`.

| Code | Name | Meaning |
|------|------|---------|
| -1 | ResourceNotFound | RID not in the process's resource table |
| -2 | Unsupported | Operation not supported by this resource |
| -3 | InvalidInput | Bad argument contents (e.g. non-UTF-8 string) |
| -4 | BadAddress | User pointer/length fails validation |
| -5 | InvalidSyscall | Unknown syscall number |
| -6 | NotPermitted | Caller lacks permission (Stage 11) |
| -7 | WouldBlock | Non-blocking mode and no data/space |
| -8 | Closed | Other end of pipe/connection is gone |
| -9 | Busy | Exclusive resource already in use |
| -10 | PathNotFound | File/dir path doesn't exist |
| -11 | AlreadyExists | Path/port already exists |
| -12 | Locked | Directory locked by another process |
| -13 | OutOfMemory | Allocation failed |

Convention: `read` returning `0` means EOF / end-of-stream (for blocking resources,
`read` blocks rather than returning 0 unless the stream is finished).

## Pre-existing Bugs To Fix (assigned to stages)

These were found by code inspection while writing these plans; each is owned by a
stage:

1. **Unchecked syscall enum (UB):** `kernel/src/processes/syscalls/mod.rs` declares
   `extern "C" fn syscall(syscall: Syscall, ...)` — a raw user-controlled `u64` is
   transmuted into the enum. Invalid values are instant UB. → **Fixed in pre-Plans
   pass** (`TryFrom<u64>` dispatch, -5 on unknown).
2. **No user-pointer validation:** `sys_read`/`sys_write` build slices from raw user
   pointers; a process can pass a kernel address and read/write kernel memory.
   → Stage 2.
3. **Wrong key type:** `Process.resources` is `IDTable<ProcessID, ...>`
   (`kernel/src/processes/process.rs:22`); should be `IDTable<ResourceID, ...>`
   (same underlying `u32`, so it compiles). → **Fixed in pre-Plans pass**.
4. **Graveyard kstack use-after-free (REFUTED):** interrupt handlers run on a shared
   IST stack (`PIC_STACK`), *not* on the interrupted thread's kstack. The graveyard
   drain in `schedule()` is therefore safe — it frees kstacks of threads whose CPU
   state has already been saved elsewhere, never the stack the handler is currently
   running on. No reaper is needed for this specific hazard. → Stage 4 reaper design
   updated accordingly.
5. **`wee_alloc` is unmaintained/archived** and the userspace heap can't grow.
   → Stage 3 (shared free-list allocator crate).
6. **Storage/filesystem lifetime dead-end:** the FAT mount in `kmain` lives inside a
   throwaway closure; nothing can touch the disk after boot. `FileSystem<'a>` borrows
   its `BlockDevice`. → Stage 5 (owned devices + global VFS).

## Design Decisions Already Made (do not re-litigate)

- **`spawn` first, `fork` later.** `spawn`+pipes+handle-passing is the Stage I process
  model (Stage 7); `fork`+CoW is Stage 12.
- **Console takes over the framebuffer.** Opening the FB device detaches the kernel's
  `FbTerminal`; the kernel log keeps flowing to serial + the klog ring buffer.
  Closing the FB resource reattaches the kernel terminal. (Stage 6)
- **No `wait` syscall** (not in the design doc). Parents poll `ProcInfo` (+`Sleep`)
  until the child PID disappears. `libsystem` provides a `wait_for_exit(pid)` helper.
- **IPC ports are string-named** (e.g. `"system.registry"`), matching the
  `driver:device` string-identifier style already used by `DeviceManager`. (Stage 9)
- **`Delete` syscall is an addition** beyond the doc's syscall table (the doc has
  `move`/`mkdir` but no way to remove anything; a shell needs it). Flagged as a
  deliberate extension.
- **Devices stay out of the filesystem** (`lsdev`/`opendev`), per the doc.
- **`libsystem` *is* the doc's "libsys"** — one userspace library crate; the registry
  client API is added to it in Stage 10 rather than creating a second crate.

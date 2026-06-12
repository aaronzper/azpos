# Stage 7 — spawn, Pipes, Args & Handle Passing

## Goal

Processes can create other processes: `Spawn` loads an ELF from a VFS path with
string arguments and explicitly passed resource handles; `Pipe` provides
bidirectional streams. This is the Stage I process model (fork comes much later, in
Stage 12) and the substrate for console↔shell plumbing in Stage 8.

## Prerequisites

Stages 4 (exit/teardown), 5 (VFS read), 6 (`SetMode` semantics for non-blocking).

## Current State

- `spawn_proc(name, elf_data)` (`kernel/src/processes/mod.rs`) works: validates ELF,
  creates `Process`, loader thread maps segments + 4-page stack, `iretq`s to ring 3.
  Entry receives nothing (no args); only the kernel calls it (for init).
- `libsystem::runtime::_start` calls `extern "C" main()` with no arguments.
- Resources live per-process as `Box<dyn Resource + Send>`; nothing can be
  transferred between processes.
- `IDTable` allocates IDs — **verify it allocates sequentially from 0** (the
  handle-passing convention below depends on a fresh table yielding 0,1,2,…; if it
  doesn't, add an insertion method that does).

## Design Decisions

1. **`Pipe() -> packed pair`** (syscall 30): creates a *bidirectional* stream (per
   the doc) = two unidirectional ring buffers. Kernel object:
   `PipeBuffer { ring: VecDeque<u8> (cap 4096), closed_read: bool, closed_write: bool, mtx: KMutex, readable: KCondvar, writable: KCondvar }`.
   Each end is a `PipeResource { rx: Arc<PipeHalf>, tx: Arc<PipeHalf>, nonblock: bool }`
   (end A reads buf1/writes buf2; end B the reverse). Semantics:
   - `read`: blocks until ≥1 byte or all writers gone → returns 0 (EOF). Non-block:
     `-7`.
   - `write`: blocks while full; if no readers → `-8 Closed`. Non-block: partial
     write or `-7`.
   - `Drop` marks the half closed and notifies both condvars.
   Return packing: `rax = (rid_b << 32) | rid_a` — both RIDs are `u32` and the
   resource table will never realistically reach `2^31`, so the i64 stays positive;
   `libsystem` unpacks to `(ResourceID, ResourceID)`.
   `ResourceType::Pipe`; `get_info` → `ResourceInfo::Pipe { buffered: u64, peer_closed: bool }`.
2. **`Spawn(req_ptr, req_len) -> pid`** (syscall 29): request is postcard-encoded
   libsci type:
   ```rust
   pub struct SpawnRequest {
       pub path: String,            // absolute VFS path to ELF
       pub args: Vec<String>,
       pub handles: Vec<ResourceID> // caller's RIDs to grant the child
   }
   ```
   Kernel: validate + decode (`-3` on bad postcard), read ELF via VFS (`-10` if
   missing), then extend `spawn_proc`:
   - **Handle passing is by clone:** add `fn try_clone(&self) -> Option<Box<dyn Resource + Send>>`
     to the kernel-side resource plumbing. Implementations: `PipeResource` → clones
     Arcs (this is the important one); `LoggerResource` → trivial clone;
     `BlobResource`/`FileResource`/device resources → `None` for now (return `-2`
     from `Spawn` if a non-cloneable handle is listed; `FileResource` cloning can be
     added when needed; device handles are exclusive by design).
     Note: `Resource` is a libsci trait shared with userspace — put `try_clone` in a
     kernel-local extension trait (`KernelResource: Resource`) rather than libsci,
     and make the process table hold `Box<dyn KernelResource + Send>`. Userspace's
     `SystemResource` is unaffected.
   - Clones are inserted into the child's fresh table **in `handles` order**, so they
     get RIDs `0, 1, 2, …`. **Convention (document in libsci):** handle 0 = stdin,
     handle 1 = stdout, by agreement between parent and child; the kernel doesn't
     interpret them.
   - **Args delivery via the stack:** serialize `Vec<String>` (postcard) and copy it
     to the top of the child's user stack (16-byte aligned down); set the entry
     frame's `rdi = ptr`, `rsi = len`, and initial `rsp` below it. The loader thread
     in `spawn_proc` already builds the `InterruptStackFrameValue` — adjust there.
3. **Runtime update:** `libsystem::runtime::_start(rdi, rsi)` decodes args and calls
   `fn main(args: Args)` where `Args` is a thin libsystem type
   (`Vec<String>` + accessors). **This changes the signature of every program's
   `main`** — update `adam` and `ktest` in this stage. Also: `_start` now calls
   `sys_exit(0)` when `main` returns (replaces `loop {}`), and the panic handler
   becomes: write the panic message via the logger resource if possible, then
   `sys_exit(101)`.
4. **Process naming:** `Spawn` names the child from the path's file stem; `ProcInfo`
   exposes it.
5. **No wait syscall** (design decision, see overview): `libsystem::wait_for_exit`
   (Stage 4) is the blessed pattern. Spawn returns the PID; the PID is immediately
   probeable via `ProcInfo`.

## Work Items

1. `PipeBuffer`/`PipeResource` + `sys_pipe` + packing/unpacking + `SetMode` support.
2. `KernelResource` extension trait + table type change + `try_clone` impls.
3. `SpawnRequest` (libsci) + `sys_spawn` (decode, VFS read, clone handles) +
   `spawn_proc` extension (pre-seeded resource table, args-on-stack, name).
4. Runtime change (`Args`, auto-exit, panic→exit) + update `adam` & `ktest` mains +
   libsystem ergonomic `process::{spawn, wait_for_exit}`.
5. New test helper program `programs/ktest-child/`: behavior driven by `args[0]`:
   - `echo`: read stdin (handle 0) until EOF, write everything to stdout (handle 1)
     uppercased, exit 0.
   - `args`: write its args joined by `,` to stdout, exit 0.
   - `exitcode`: exit with code `args[1]`.
   - `lockfail`: try to `OpenFile` a path given in `args[1]`, write the resulting
     error code to stdout.
   - `hang`: loop forever (for `Kill` testing).
   build.rs: add to both images as `/Programs/ktest-child.exe`.
6. ktest additions:
   - `pipe_roundtrip`: pipe pair, write A→read B and B→A, data matches.
   - `pipe_eof_and_closed`: close A; read on B drains then returns 0; write on B
     → `-8`.
   - `pipe_nonblock`: empty + nonblock read → `-7`; fill 4096 bytes, nonblock write
     → `-7` or partial.
   - `spawn_args`: spawn `ktest-child args a b c` with a pipe as its stdout; read
     "args,a,b,c"; `wait_for_exit`.
   - `spawn_echo_pipes`: stdin+stdout pipes, write "hello", close write end, read
     "HELLO", EOF, child exits.
   - `spawn_missing`: path `/programs/nope.exe` → `-10`.
   - `kill_child`: spawn `hang`, `ProcInfo` shows it, `Kill`, `wait_for_exit`
     completes, `Sysinfo.n_procs` back to baseline (**this is the deferred Stage 4
     reclamation test — assert repeated spawn/kill ×20 keeps `n_procs` and
     `heap_used_bytes` from growing monotonically**, catching teardown leaks).
   - `lockdir_cross_process`: `LockDir /test-data`, spawn `lockfail` child targeting
     a file inside it → child reports `-12`; unlock, spawn again → child reports
     success. (Completes the Stage 5 deferred test.)

## Deliverables

- `Pipe` + `Spawn` syscalls with args & handle passing; stdin/stdout convention.
- Updated userspace runtime (args, auto-exit, panicking processes die cleanly).
- `ktest-child` multipurpose test program; cross-process tests now possible.

## Completion Criteria

1. `cargo run -- -test` passes everything, including `kill_child`'s
   20-iteration leak check and the cross-process lock test.
2. A panicking child (add `ktest-child panic` mode) exits with code 101 and the
   parent's `wait_for_exit` returns — no kernel panic, no hung VM.
3. Normal `cargo run`: adam unchanged in behavior (updated main signature only).

## Out of Scope

- `fork` (Stage 12). Environment variables, CWD (not in the design).
- Passing file/device handles (`try_clone` returns `None`; extend when a real need
  appears — likely Stage 11 FB sharing, which passes the fb handle: **note** that
  Stage 11 will need `try_clone` on `FbResource` to be a *move* instead — design
  there).
- Job control, signals, exit-code retrieval (roadmap).

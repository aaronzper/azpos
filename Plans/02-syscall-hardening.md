# Stage 2 — Syscall Layer Hardening & Resource Trait Completion

## Goal

Make the syscall boundary safe against hostile/buggy userspace (no UB, no kernel
memory access via user pointers), finish the "general resource" syscall set from the
design doc (`seek`, `get_type`, `get_info`), and clean up the resource plumbing that
every later stage builds on.

## Prerequisites

Stage 1 (test harness).

## Current State

- ~~`kernel/src/processes/syscalls/mod.rs`: `extern "C" fn syscall(syscall: Syscall, ...)`
  — the raw `rdi` value is reinterpreted as the enum (**UB for invalid values**), and
  unknown variants hit `panic!` (user can panic the kernel).~~ **Fixed in pre-Plans
  pass**: dispatch is now `fn syscall(nr: u64, ...)` with `Syscall::try_from(nr)`;
  unknown numbers return -5 (`InvalidSyscall`).
- `sys_read`/`sys_write` (`handlers.rs`) build slices straight from user pointers —
  **no validation**; a process can pass `0xFFFF_8000_...` and read/write kernel memory.
- `sys_seek` is `todo!()` (kernel panic if called).
- ~~`Process.resources` is `IDTable<ProcessID, Box<dyn Resource + Send>>`
  (`process.rs:22`) — key type should be `ResourceID`.~~ **Fixed in pre-Plans pass**.
- `DeviceManager::open_device` returns `Box<dyn Resource>` without `Send`
  (`devices/manager.rs:79`) — incompatible with the process resource table.
- Every handler repeats the same lock dance: `SCHEDULER.lock().current_proc()` then
  `PROCESSES.lock().get_entry_mut(pid)`.
- `libsci::resources`: `Resource` trait has `read`/`write`/`seek`; error codes
  -1/-2/-3 + `Misc(<= -0x10000)` exist; `rax_to_result` doesn't decode -3 (bug).

## Design Decisions

1. **Dispatch on `u64`, never on `Syscall` directly.** Implement
   `impl TryFrom<u64> for Syscall` in `libsci` (manual `match` with the explicit
   numbers from `00-OVERVIEW.md`). The kernel entry fn signature becomes
   `extern "C" fn syscall(nr: u64, arg1: u64, arg2: u64, arg3: u64) -> i64`; failure
   returns `-5` (`InvalidSyscall`). Remove the `panic!` arm.
2. **Error codes:** extend `ResourceError` (libsci) with `BadAddress=-4`,
   `InvalidSyscall=-5`, `NotPermitted=-6`, `WouldBlock=-7`, `Closed=-8`, `Busy=-9`,
   `PathNotFound=-10`, `AlreadyExists=-11`, `Locked=-12`, `OutOfMemory=-13`, per the
   overview table. Fix `rax_to_result` to decode every standard code (and stop
   panicking on unknown negatives in the reserved range — map them to `Misc`).
3. **User pointer validation** lives in a new module
   `kernel/src/processes/syscalls/usermem.rs`:
   - `fn user_slice(ptr: u64, len: u64) -> Result<&'static [u8], ResourceError>` and
     `user_slice_mut` — validate: no `ptr+len` overflow; entire range
     `< USER_END_ADDR`; every page in the range translates via `current_pt()` and is
     `USER_ACCESSIBLE` (and `WRITABLE` for the mut variant). Return `BadAddress`
     otherwise. (The process's lower half is the live page table during a syscall, so
     borrowing directly after validation is sound on this single-CPU kernel. SMAP/SMEP
     are not enabled — note this in a doc comment as a future hardening item.)
   - `fn user_str(ptr: u64, len: u64) -> Result<&'static str, ResourceError>` —
     `user_slice` + UTF-8 check (`InvalidInput`), cap length at 4 KiB.
   All handlers taking pointers go through these.
4. **`with_current_proc` helper** in `syscalls/mod.rs`:
   `fn with_current_proc<R>(f: impl FnOnce(&mut Process) -> R) -> Option<R>` — reads
   the PID (drops the `SCHEDULER` lock before touching `PROCESSES`, preserving the
   global lock order), then locks `PROCESSES` and runs `f`. Rewrite all handlers to
   use it.
5. **Resource trait additions** (in `libsci`, since userspace's `SystemResource`
   implements the trait too):
   ```rust
   fn get_type(&self) -> ResourceType;
   fn get_info(&self) -> ResourceInfo;          // kernel-side impls
   ```
   New serialized types in `libsci::resources`:
   - `#[non_exhaustive] enum ResourceType { Logger=0, Blob=1, File=2, Device=3, Pipe=4, IpcListener=5, IpcConnection=6 }`
     (plain enum; `GetType` returns the discriminant in `rax`). Variants beyond
     `Blob` are declared now, used by later stages.
   - `enum ResourceInfo` (postcard-serialized, append-only): variants per type, e.g.
     `Blob { size: u64, pos: u64 }`, `Logger`, others added by later stages.
   - `GetInfo(rid, buf_ptr, buf_len)` ABI: kernel serializes `ResourceInfo`, copies
     `min(buf_len, serialized_len)` bytes into the user buffer, and **returns the
     full serialized length**. Caller retries with a bigger buffer if the return
     value exceeds `buf_len`. (This buffer convention is reused by `ProcInfo`/`Sysinfo`
     in Stage 4.)
   For userspace `SystemResource`, implement `get_type`/`get_info` via the new
   syscalls (`get_info` returning the decoded `ResourceInfo`).
6. **Send-ness:** change `DeviceManager::open_device` (trait + impls) to return
   `Option<Box<dyn Resource + Send>>`. Fix `Process.resources` key type to
   `ResourceID`.

## Work Items

1. ~~libsci: explicit discriminants on existing `Syscall` variants (0–6)~~ **Done
   (pre-Plans pass)**. Remaining: add `GetType = 7`, `GetInfo = 8` with full rustdoc
   (args, returns, buffer convention). `TryFrom<u64>` **done**. New error variants +
   fixed `rax_to_result`/`result_to_rax` (exhaustive in both directions).
2. libsci: `ResourceType`, `ResourceInfo`, trait additions. Update kernel impls
   (`LoggerResource`, `BlobResource`) and userspace `SystemResource`.
3. Kernel: `usermem.rs` with validation helpers + rustdoc on the soundness argument.
4. Kernel: rewrite `syscalls/mod.rs` dispatch (`u64` + `TryFrom`), route `Read`/
   `Write`/future pointer args through `usermem`, implement `sys_seek` (dispatch to
   `resource.seek(offset)`), `sys_get_type`, `sys_get_info`.
5. Kernel: `with_current_proc`; refactor `handlers.rs`; fix the resource-table key
   type; fix `open_device` signature (the keyboard driver's `todo!()` body stays —
   Stage 6).
6. ktest additions:
   - `invalid_syscall_rejected`: raw syscall `9999` returns `-5` (expose a
     `pub fn make_raw_syscall(nr: u64, ...) -> i64` from `libsystem::syscalls` for
     tests).
   - `kernel_ptr_read_rejected`: `Read` with `ptr = 0xFFFF_8000_0000_0000` returns
     `-4`. Also `Write` with a huge `len` that overflows → `-4`.
   - `unmapped_ptr_rejected`: `Read` into an unmapped user address (e.g.
     `0x7000_0000_0000`) returns `-4`.
   - `blob_seek_works`: `ListDevices` blob — read 4 bytes, `Seek(0)`, read 4 bytes,
     compare equal.
   - `get_type_get_info`: logger reports `Logger`; blob reports `Blob` and
     `get_info` round-trips `{size, pos}` correctly including the
     short-buffer-then-retry path.

## Deliverables

- No code path where userspace input can cause kernel UB or panic (grep audit:
  `unwrap`/`panic!`/`todo!` reachable from `syscall()` with user-controlled input —
  the remaining `current_proc().unwrap()`s are fine, a syscall always has a proc).
- Full general-resource syscall set: `read`, `write`, `seek`, `close`, `get_type`,
  `get_info`.
- `usermem` validation used by every pointer-taking syscall.

## Completion Criteria

1. `cargo run -- -test` passes with all Stage 1 + Stage 2 tests
   (`DONE passed=8 failed=0`).
2. The three hostile-input tests pass — i.e. the kernel survives invalid syscall
   numbers, kernel-space pointers, overflowing lengths, and unmapped pointers, and
   returns the documented error codes.
3. `cargo run` normal boot unchanged (manual).

## Out of Scope

- SMAP/SMEP enablement (documented as future hardening in `usermem.rs`).
- Blocking-mode flags (`SetMode`) — Stage 6.
- Any new resource kinds — later stages.

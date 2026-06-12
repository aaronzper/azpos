# Stage 4 — Time & Process Lifecycle (sleep, exit, kill, pinfo, klog, sysinfo, power)

## Goal

Give the kernel a time base and complete the process-syscall set short of `spawn`:
`Time`, `Sleep`, `Exit`, `Kill`, `GetPid`, `ProcInfo`, plus the misc syscalls
`Klog`, `Sysinfo`, `Power`. Fix the thread-graveyard stack bug. After this stage a
process can end, and all its memory/resources are reclaimed.

## Prerequisites

Stages 2 (error codes, buffer convention) and 3 (`free_user_half`).

## Current State

- Timer: PIT at its power-on default (~18.2 Hz), used only to drive `schedule()`
  (`interrupts/handlers.rs::timer_inner`). No tick counting, no wall clock.
- `thread_exit` exists (kernel threads); processes can never end — adam/ktest loop
  forever; `Process` entries, resources, and user pages are never reclaimed.
- **Graveyard design note (overview bug #4 — refuted):** interrupt handlers run on a
  shared IST stack (`PIC_STACK`), not on the interrupted thread's kstack. The existing
  graveyard drain in `schedule()` is therefore safe — it frees kstacks of threads whose
  CPU state lives in the scheduler's `threads` table, not on the IST stack. A dedicated
  reaper thread is not required for correctness, but is still useful for separation of
  concerns (process teardown work runs on its own stack instead of inside the timer
  IRQ). The reaper design below is kept for that reason.
- Logger (`kernel/src/logger.rs::_log`) writes to serial + FbTerminal only; nothing
  retains log history for userspace.
- `Scheduler` has `block_thread` → `BlockedThread` RAII (unblocks on drop) — reuse it
  for sleeping.

## Design Decisions

1. **Tick base:** program PIT channel 0 to 1000 Hz (divisor 1193) in a new
   `devices/pit.rs`, called from `init_interrupts`. Global
   `static TICKS: AtomicU64`, incremented in `timer_inner` *before* the scheduler
   try_lock (so ticks advance even when the scheduler is held).
   `pub fn uptime_ms() -> u64`.
2. **Wall clock:** CMOS RTC driver (`devices/rtc.rs`): read seconds/minutes/…/year
   registers with the read-until-stable + BCD/binary-mode handling; convert to a Unix
   timestamp (write the days-since-epoch math by hand; no chrono). Read once at boot
   into `BOOT_UNIX_TIME`; `Time` syscall returns
   `BOOT_UNIX_TIME + uptime_ms()/1000` (u64 seconds).
3. **Reaper fixes the graveyard bug:** `schedule()` no longer drops threads. Instead
   it moves dead `Thread` objects into a `REAPER_QUEUE: KIntMutex<Vec<Thread>>`, and
   a dedicated kernel thread (`reaper`, spawned in `kmain`) drains that queue and
   drops them — the reaper runs on its own stack, so freeing other threads' kstacks
   is safe. The reaper parks on a `KCondvar` (or polls with `Sleep` once it exists);
   `schedule()` must only push + notify, never drop.
4. **Sleep:** `Sleep(ms)`. Scheduler gains
   `sleeping: Vec<(wake_tick: u64, BlockedThread)>`; `sys_sleep` computes
   `wake_tick = TICKS + ms`, blocks the current thread, pushes the entry, yields. At
   the top of `schedule()`, drain entries with `wake_tick <= TICKS` (dropping the
   `BlockedThread` unblocks). Accuracy: ±1 tick is fine; never wake early.
5. **Process teardown** — one function used by both `Exit` and `Kill`:
   `pub fn terminate_proc(pid: ProcessID)` in `processes/mod.rs`:
   1. Remove the `Process` from `PROCESSES` (drops its resource table → resource
      `Drop` impls run; directory locks release here in Stage 5).
   2. `free_user_half(&proc)` (Stage 3). If `pid` is the *currently loaded* process
      (it is, for `Exit`), first zero the live lower-half L4 entries + TLB flush so
      the freed frames are unreachable.
   3. Kill every scheduler thread tagged with `pid` (today: exactly one; keep it a
      loop — Stage 12 adds more).
   `sys_exit(code)`: log `[proc <pid> '<name>'] exited code=<code>` via `println!`,
   call `terminate_proc(current)`, then `thread_yield()` in a loop — the handler must
   never return to userspace (its mappings are gone). The dying thread's kstack is
   freed by the reaper, never by itself.
   `sys_kill(pid)`: `ResourceNotFound` if no such pid; otherwise `terminate_proc`.
   (No permission check until Stage 11.) Killing *yourself* via `Kill` must behave
   like `Exit`.
   **Exit code storage: none** (no `wait` in the design). Liveness is probed via
   `ProcInfo` → `PathNotFound`-style `-1`.
6. **ProcInfo:** `ProcInfo(pid, buf_ptr, buf_len)` using the Stage 2 buffer
   convention. New libsci type
   `ProcessInfo { pid: u32, name: String, n_threads: u32, user: u32 }` (postcard;
   `user` is 0 until Stage 11). `GetPid()` returns the caller's PID in `rax`.
7. **Klog ring buffer:** `logger.rs` gains `KLOG: KIntMutex<KlogRing>` — 64 KiB byte
   ring storing everything `_log` writes (plus a monotonically increasing
   `total_written: u64`). `Klog()` syscall → `KlogResource` (new
   `ResourceType::Blob`? **No** — new `ResourceType` variant `Klog=7`): tracks a
   per-reader offset (starting at the oldest retained byte); `read` is
   **non-blocking**: copies available bytes, returns 0 when caught up (readers poll
   with `Sleep`; rationale: `_log` runs in interrupt contexts where waking a condvar
   is not safe). If the reader fell behind the ring, skip forward and prepend the
   literal text `\n[klog: <n> bytes dropped]\n`.
8. **Sysinfo:** buffer-convention syscall; libsci
   `SystemInfo { os_name: String, version: String, uptime_ms: u64, phys_mem_bytes: u64, heap_used_bytes: u64, n_procs: u32 }`
   (version from `env!("CARGO_PKG_VERSION")` of the kernel crate).
9. **Power:** `Power(action)`: `0` = shutdown — QEMU q35 ACPI: `outw(0x604, 0x2000)`;
   `1` = reboot — 8042 pulse (`outb(0x64, 0xFE)`), fall back to triple fault.
   Anything else `InvalidInput`. (Real-hardware ACPI is roadmap.)
10. **ktest now exits:** replace the trailing `loop {}` with `sys_exit(0)`. The
    runner keeps the marker-based verdict (DONE then QEMU killed) — but after this
    stage, also accept the VM shutting itself down cleanly: ktest's last act after
    `DONE` is `Power(0)`. Runner: treat QEMU exiting with success after `DONE` as
    pass (this removes the need to kill QEMU).

## Work Items

1. `devices/pit.rs` (+ init), `TICKS`, `uptime_ms`. `devices/rtc.rs` + boot-time
   read.
2. Reaper thread + scheduler graveyard refactor. Audit: the existing
   ELF-loader-thread exit path (`thread_exit` after `iretq`? no — the loader thread
   never returns; but the *mount* closure thread in `kmain` does exit) now goes
   through the reaper.
3. Scheduler `sleeping` list + `sys_sleep`.
4. `terminate_proc`, `sys_exit`, `sys_kill`, `sys_getpid`, `sys_procinfo`.
5. `KlogRing` + `KlogResource` + `sys_klog`. `LoggerResource` unchanged (it's the
   write side; klog is the read side).
6. `sys_sysinfo`, `sys_power`. Libsci: syscalls 11–19 with explicit discriminants +
   rustdoc; libsystem wrappers for all; `libsystem::wait_for_exit(pid)` helper
   (`ProcInfo` poll + `Sleep(10)` loop).
7. Runner: accept clean-shutdown-after-DONE as pass.
8. ktest additions:
   - `time_monotonic`: two `Time` calls 1100 ms apart (via `Sleep`) differ by ≥1.
   - `sleep_duration`: `uptime` delta across `Sleep(200)` is ≥200 and <1000 ms
     (uptime via `Sysinfo`).
   - `getpid_stable`: `GetPid` returns the same nonzero value twice.
   - `procinfo_self`: `ProcInfo(self)` decodes; name == "init"; `ProcInfo(9999)`
     → `-1`.
   - `klog_sees_logger_writes`: write a sentinel via logger resource, then read the
     klog resource until the sentinel appears (bounded loop).
   - `sysinfo_sane`: decodes; `phys_mem_bytes > 0`, `n_procs >= 1`.
   - Final: `DONE`, then `Exit`… actually `Power(0)` (exit-then-power can't run;
     `Power(0)` last, after `DONE`).
   - Plus a *kernel-driven* check that exit reclaims: before `DONE`, spawn nothing —
     instead assert via `Sysinfo.n_procs` (should be 1). Real exit-reclaim testing
     becomes possible in Stage 7 when `Spawn` exists; add a TODO marker in ktest.

## Deliverables

- 1 kHz tick + RTC wall clock; `Time`/`Sleep` syscalls.
- Safe thread reaping (graveyard bug fixed).
- Full process teardown reclaiming resources, user pages, and threads.
- `Exit`/`Kill`/`GetPid`/`ProcInfo`/`Klog`/`Sysinfo`/`Power` implemented, wrapped,
  documented.

## Completion Criteria

1. `cargo run -- -test` passes; the run ends by the **guest powering itself off**
   after `DONE` (no 60 s timeout, no runner kill needed).
2. `sleep_duration` and `time_monotonic` pass — PIT reprogramming verified.
3. Boot a normal `cargo run`: adam still runs; after this stage adam also ends with
   `Exit(0)` (update adam) and serial shows the exit line with its PID — visually
   confirms teardown of a real process. No panic from the reaper (the old graveyard
   crash would appear here).
4. Soak: ktest runs `Sleep(1)` in a 500-iteration loop (add `sleep_soak` test) —
   passes, proving the reaper/sleep lists don't corrupt scheduling.

## Out of Scope

- `spawn` (Stage 7) — so no parent/child relationships yet.
- Permission checks on `Kill`/`SetUser` (Stage 11).
- APIC/HPET timers, tickless idle (roadmap).
- Exit-code delivery to parents (no `wait` by design; revisit on the roadmap).

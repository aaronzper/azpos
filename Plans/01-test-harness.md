# Stage 1 — Automated QEMU Test Harness

## Goal

A single command — `cargo run -- -test` — that boots a test image headlessly in QEMU,
runs a userspace test program, and exits 0/nonzero based on serial-output markers.
Every later stage adds tests to this harness; it is the definition of "done" for the
whole plan series.

## Prerequisites

None. This is the first stage.

## Current State

- `src/main.rs` (root crate) is the QEMU runner; flags: `-uefi`, `-gdb`. Serial goes
  to stdio, QEMU runs with a display window.
- `build.rs` (root crate) builds the kernel + `adam` via artifact dependencies,
  creates `bios.img` (MBR, 256 MiB, FAT32 data partition containing
  `/Programs/adam.exe`) and `uefi.img` (no data partition — known limitation).
- `kernel/src/main.rs:80` hardcodes spawning `/programs/adam.exe`.
- `kernel/src/panic.rs` has the panic handler (prints, halts).
- There is no exit syscall yet, so test programs cannot terminate the VM; the harness
  must detect completion via serial markers and kill QEMU itself.

## Design Decisions

1. **Init-by-convention:** the kernel spawns `/programs/init.exe` (rename from
   `adam.exe`). Which binary *is* init is decided at image-build time:
   - `bios.img` → `adam` as `/Programs/init.exe`
   - `test.img` (new, BIOS-style) → `ktest` as `/Programs/init.exe`
   This avoids cargo-feature plumbing through artifact dependencies (features would
   unify across both kernel builds).
2. **Marker protocol** (grep-able, stable — later stages depend on these exact
   strings):
   - `[KTEST] <name> PASS` / `[KTEST] <name> FAIL <detail>` per test case
   - `[KTEST] DONE passed=<n> failed=<m>` as the final line
   Markers are written through the logger resource (`GetLogger` → `Write`), which
   reaches serial via the kernel log.
3. **isa-debug-exit for panics:** the kernel panic handler writes `0x10` to port
   `0xf4` after printing the panic. In a normal run (no `isa-debug-exit` device) the
   port write is a no-op; in test runs QEMU exits immediately with code
   `(0x10 << 1) | 1 = 33`, which the runner reports as "kernel panicked".
4. **The runner owns pass/fail logic** (not a shell script): it spawns QEMU with
   piped stdout, echoes serial lines, and decides the verdict. Timeout: 60 s.

## Work Items

1. **Kernel: generic init path.** In `kernel/src/main.rs`, change the spawned path to
   `/programs/init.exe`. (FAT name matching is case-insensitive via the existing
   uppercase logic in `filesystem/fat/mod.rs`.)
2. **Kernel: panic exit hook.** In `kernel/src/panic.rs`, after printing the panic,
   write `0x10u32` to port `0xf4` (use the existing `devices::write_port` helper),
   then halt as today.
3. **New crate `programs/ktest`:** copy `programs/adam`'s structure (links
   `libsystem`). `main` runs a list of test functions, each printing the marker lines
   via the logger resource. Initial tests (current functionality only):
   - `logger_roundtrip`: `GetLogger`, `Write` returns the byte count.
   - `list_devices_parses`: `ListDevices`, read full blob (loop like current adam),
     `postcard::from_bytes::<Box<[DriverInfo]>>` succeeds, ≥1 driver.
   - `close_then_use_fails`: `Close` the logger RID, then `Write` to it returns `-1`.
   After `DONE`, loop forever (no exit syscall yet; Stage 4 will replace this with
   `Exit`).
4. **build.rs: test image.** Add `ktest` as an artifact dependency in the root
   `Cargo.toml`. Refactor `create_root_fs` to take the init binary path as a
   parameter. Build a third image `test.img`: same MBR/FAT32 layout as `bios.img`,
   with `ktest` as `/Programs/init.exe`. Also write `adam` into both images as
   `/Programs/adam.exe` (keeps a second known file on disk for later FS tests).
   Export `cargo:rustc-env=TEST_PATH=...`.
5. **Runner: `-test` mode.** In `src/main.rs`:
   - Use `test.img`, add `-display none`,
     `-device isa-debug-exit,iobase=0xf4,iosize=0x04`, keep `-serial stdio` but
     capture it (`Stdio::piped()`).
   - Read lines, echo each to the runner's stdout, track `[KTEST]` markers.
   - Verdict: exit 0 iff a `DONE` line with `failed=0` was seen. Kill QEMU once
     `DONE` is seen. Nonzero on: any `FAIL`, `failed>0`, QEMU exit code 33 (panic),
     or 60 s without `DONE` (print which tests were seen before the timeout).
6. **Docs:** update `CLAUDE.md` Build & Run section with `cargo run -- -test` and the
   marker protocol; note that all future syscall work must extend `ktest`.

## Deliverables

- `programs/ktest/` crate with 3 passing tests.
- `test.img` built alongside the existing images.
- `cargo run -- -test` headless run with correct exit codes.
- Kernel boots init by `/programs/init.exe` convention; panics exit QEMU in test mode.

## Completion Criteria (verify all)

1. `cargo run -- -test` exits 0 and prints three `PASS` markers and
   `[KTEST] DONE passed=3 failed=0`.
2. Temporarily make `logger_roundtrip` assert a wrong byte count → `cargo run -- -test`
   exits nonzero. Revert.
3. Temporarily add `panic!()` early in `kmain` → `cargo run -- -test` exits nonzero
   within seconds (not via timeout), reporting a kernel panic. Revert.
4. `cargo run` (normal mode) still boots to the framebuffer terminal and runs adam
   exactly as before (manual check).
5. `cargo run -- -uefi` still reaches the kernel banner (manual check; adam won't
   spawn on UEFI — pre-existing limitation, unchanged).

## Out of Scope

- UEFI data partition (GPT surgery on the bootloader's image) — roadmap.
- Kernel-internal unit tests / `#[test]` harness — userspace ktest exercising
  syscalls is the test vehicle for everything.
- Sending keyboard input to QEMU from the runner (QMP `sendkey`) — revisit in Stage 8
  if console testing demands it.

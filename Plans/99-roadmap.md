# Post-Stage-II Roadmap

Not plans — a parking lot of designed-enough directions for after Stage II, in
rough dependency/value order. Promote an item to a numbered plan file when its
time comes, using the same template (Goal / Prerequisites / Current State /
Design Decisions / Work Items / Deliverables / Completion Criteria).

## Deferred Items From Stages 1–12 (small, do opportunistically)

- UEFI data partition (GPT) so userspace runs on UEFI boots; then run the test
  suite on both firmwares.
- SMAP/SMEP enablement (hardening note in `usermem.rs`).
- Allocator: return freed pages to the page allocator; consider slab/buddy.
- VFS: block cache, finer-grained locking, open-file coordination
  (delete-while-open), relative paths/CWD.
- Pipes/IPC: handle transfer over IPC connections (would replace the Stage 11
  FBRUN handle-move dance with a clean grant protocol).
- `wait`-style exit-code delivery (parent-notify), replacing `ProcInfo` polling.
- File resource `try_clone`; fork sharing files.
- Registry: permission enforcement hardening, watches/subscriptions; password
  hashing if Stage 11 shipped plaintext.
- libterm ANSI escape codes + colors; shell history/completion.
- Automated console rendering tests via QMP `screendump` + image comparison, and
  synthetic input via QMP `sendkey` (makes Stages 8/11 manual checklists CI-able).
- Scancode set 2 / keyboard LEDs; mouse scroll wheel.

## Smarter Scheduling

Replace least-runs round-robin: per-thread priorities + sleep-aware accounting
(the current `runs` counter starves nothing but rewards sleepers oddly), MLFQ or
stride scheduling. Prereq for: responsive console under fork-stress. Also: tickless
idle (HLT with a programmed one-shot deadline — needs APIC timer), and eventually
APIC/SMP (big: per-CPU schedulers, IPIs, real spinlock discipline — the `KIntMutex`
single-CPU assumptions are load-bearing today; grep for them first).

## ext2 (second filesystem → VFS proves itself)

Read-only first (superblock, block groups, inodes, dirent iteration), then write.
Brings real file ownership/permissions — revisit Stage 11's file-permission gap.
Mount at `/ext2` initially via a second disk image attached by the runner
(`-drive` flag in `src/main.rs`). The `FileSystem` trait will need inode-ish
generalization (path-based today); expect a trait revision.

## Networking (Ethernet)

Per the design doc this is a learning goal — implement the stack, don't vendor it:
1. Driver: e1000 (QEMU default, well-documented) — TX/RX descriptor rings, IRQ.
2. Stack (own implementation): Ethernet framing → ARP → IPv4 → ICMP (ping
   first!) → UDP → minimal TCP last.
3. Userspace: socket-flavored resources (`ResourceType` additions),
   `programs/ping`.
   Test harness: QEMU user networking + `hostfwd`; ktest pings the gateway and
   echoes UDP against a runner-side socket.

## USB

xHCI controller (QEMU q35 has one) → device enumeration → HID boot-protocol
keyboard/mouse (replaces PS/2 path on real hardware) → mass storage (a second
`BlockDevice` impl behind the existing trait). Large; consider after networking.

## Port Doom

The capstone demo. doomgeneric needs: framebuffer blit (have), key events (have),
millisecond clock (have: `Sysinfo.uptime_ms` — add a cheap `Time`-ms variant or
extend `Time`), file read for the WAD (have), `malloc` (have), and a C toolchain
story: compile doomgeneric with clang `--target=x86_64-unknown-none` + a tiny
libc shim over libsystem syscalls (the shim is the real work: `fopen`/`fread`/
`malloc`/`memcpy`/`printf` subset). Run it via `fbrun doom`. Completion criteria
writes itself: the demo loop renders at the shell.

## Misc Ideas (unranked)

- `lockdir` timeouts/deadlock detection; `Sysinfo` per-process memory stats;
  kernel symbolized panics (embed a symbol table); GDB stub quality-of-life
  (`-gdb` docs + .gdbinit); `cargo run -- -test` parallel QEMU sharding if the
  suite gets slow; CI workflow (GitHub Actions: host tests + KVM-less QEMU boot
  test).

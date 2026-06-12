# CLAUDE.md

azpOS: a from-scratch x86-64 OS in Rust (BIOS & UEFI), using the `bootloader` and
`x86_64` crates. Preemptive multitasking, virtual memory, user processes, syscalls.

**Read `Plans/00-OVERVIEW.md` before doing any work.** It has the staged roadmap,
the authoritative syscall/error-code tables, locking rules, known bugs, and design
decisions. Implement whatever the next unchecked stage's plan file says.

## Build & Run

Nightly Rust + QEMU required (uses unstable cargo artifact dependencies).

```sh
cargo run            # build everything + boot in QEMU (BIOS)
cargo run -- -uefi   # UEFI boot
cargo run -- -gdb    # pause for GDB on :1234
cargo run -- -test   # headless automated test suite (exists once Plans stage 1 is done)
```

`build.rs` compiles the kernel and programs, builds bootable disk images with a
FAT32 partition holding the userspace binaries; `src/main.rs` is the QEMU runner.

## Layout

```
kernel/               the kernel (#[no_std])
libraries/libsci/     shared kernel↔userspace ABI — the Syscall enum here is the ABI source of truth
libraries/libsystem/  userspace syscall wrappers + runtime
programs/             userspace programs (adam = init)
```

## Core Rules

- Syscall ABI: `rdi`=number, `rsi`/`rdx`/`r8`=args, `rax`=return (negative = error
  code). Full docs live on `libsci::Syscall` variants.
- New syscall = libsci enum variant (explicit discriminant from the overview table)
  + handler in `kernel/src/processes/syscalls/` + `libsystem` wrapper + ktest case.
- `KIntMutex` (interrupt-disabling spinlock) for anything touched from interrupt
  handlers; blocking `KMutex` otherwise. Never block holding a `KIntMutex`.
- Don't introduce `std` anywhere; don't reorder serialized enums (postcard ABI).

# azpOS
A very WIP multitasking operating system

# Current features
- PS/2 keyboard support
- FAT32 drives over AHCI
- Multitasking!
- Virtual Memory!
- User processes!

# On the horizon
- Porting Doom!

# Building & running
Requires [QEMU](https://www.qemu.org/) (`brew install qemu` on macOS). The pinned nightly
toolchain, its `rust-src`/`llvm-tools` components, and the `x86_64-unknown-none` target are
picked up automatically via `rust-toolchain.toml` when you have `rustup` installed.

```sh
cargo build   # build the kernel, adam, and disk images
cargo run     # build and boot azpOS in QEMU
```

Pass `-uefi` to boot via UEFI instead of BIOS, and `-gdb` to start QEMU paused with a GDB stub
(`-s -S`), e.g. `cargo run -- -uefi -gdb`.

# Testing
The tests run on the host and in QEMU. Each QEMU boot runs the cases of one or two suites:

- `kernel`: `ktest!` cases in the kernel crate. They run in a kernel thread of the test kernel.
- `user`: `cases!` cases in `programs/systest`. They test the syscall API.

```sh
cargo ktest                      # kernel suite + user suite on the test kernel
cargo ktest fat::                # only cases with "fat::" in the name
cargo ktest blob --full          # the cases of blob.rs (the filter also matches a module path)
cargo ktest --kernel-only        # no user suite
cargo ktest --full               # also the cases that stop the boot
cargo ktest --isolate id_table   # one boot for each case
cargo ktest blob --full -v       # show all serial output and the time of each boot
cargo ktest --list
cargo e2e [same options]         # user suite on the production kernel
cargo test-all                   # all layers, default mode
```

Use `--isolate` to find a case that depends on another case.
Run `--full` before you merge a change to the kernel.

## Rules for a case
1. Assert the behavior that the design wants, not the behavior of today.
2. Use the check macros (`check!`, `check_eq!`, `check_matches!`, `fail!`, `.or_fail()?`). Do not use `assert!` or `unwrap` on the value under test.
3. Name a case as a statement that is true when it passes.
4. A case must not depend on another case or on the order.
5. Restore global state. Free memory, close resources and wait for threads.
6. Do not print in a case that passes.
7. A case that waits uses a bounded loop of `thread_yield()`, then fails.
8. Do not change kernel code to make a case pass. Mark the case as a known bug.

Put a kernel case at the end of the file that it tests. Put a user case in a module of
`programs/systest/src/`, and add `<module>::CASES` to `MODULES` in `main.rs`.

## Known bugs
Add an attribute to a case that fails because of a known bug:

| Attribute | Use |
|---|---|
| `#[known_bug(N)]` | The case fails with a wrong value. `N` is the GitHub ticket. |
| `#[known_bug(N, crash)]` | The case panics or hangs. Only `--full` runs it. |
| `#[known_bug(unfiled)]` | The bug has no ticket yet. |
| `#[should_panic("text")]` | The panic is the correct behavior. Only `--full` runs it. |
| `#[timeout(secs)]` | The time limit of the case. |

`grep -rn "known_bug(44" .` finds all cases for ticket #44.
When a known bug case passes, the harness reports `xpass` as a failure. Remove the attribute then.

## The summary line
```
test result: ok. 40 passed; 0 failed; 6 known bugs (xfail); 3 skipped; 2 boots; finished in 9.1s
```

- `ok` or `FAILED`: the result of the session.
- `passed`: cases that pass.
- `failed`: failed cases, `xpass` cases and harness errors. Each one makes the session fail.
- `known bugs (xfail)`: known bug cases that fail as expected.
- `skipped`: cases that stop the boot. Add `--full` to run them.
- `boots`: the number of QEMU boots. A case that stops the boot adds one.

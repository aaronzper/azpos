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

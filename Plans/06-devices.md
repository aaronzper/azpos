# Stage 6 — Userspace Device Access: opendev, Keyboard, Framebuffer, Mouse

## Goal

Implement `OpenDevice` so processes get real device resources: a readable keyboard
(key events), a writable framebuffer (with kernel-terminal handoff), and a new PS/2
mouse driver. Plus `SetMode` for non-blocking reads (the console in Stage 8 needs
it). This completes the `lsdev`/`opendev` device model from the design doc.

## Prerequisites

Stage 2 (Send-correct `open_device`, `get_info` convention). Stage 4 (`Sleep`, used
by tests).

## Current State

- `DEVICE_MANAGER` (`kernel/src/devices/manager.rs`): driver list,
  `get_device("driver:device")`, `get_drivers()` → `ListDevices` blob. Only
  `KeyboardDriver` is registered; its `open_device` is `todo!()`.
- Keyboard IRQ pushes `Scancode` into `SCANCODES: Buffer<Scancode, 64>`
  (lockless SPSC with blocking pop); a kernel thread (`keyboard_listener`) pops and
  `println!`s them — this thread is the only consumer and must go away.
- Framebuffer: `FbTerminal` (kernel terminal) owns the `Framebuffer` (which wraps the
  bootloader's `&'static mut [u8]`); installed as the logger sink
  (`logger.rs::LOGGER: KMutex<Option<FbTerminal>>`). Stage 4 added the klog ring, so
  losing the screen doesn't lose history.
- No mouse driver exists (the design doc requires PS/2 mouse).
- `libsci::devices::DeviceType` has `Framebuffer, Keyboard` (postcard — append
  only).

## Design Decisions

1. **`OpenDevice(id_ptr, id_len) -> rid`** (syscall 27): identifier is the existing
   `"driver:device"` string format. Errors: `-10` unknown identifier, `-9` exclusive
   device already open. All device resources are **exclusive** (one open handle at a
   time) in this stage — matches the doc's framebuffer note and keeps input routing
   unambiguous; shared access is a Stage 11 concern (console multiplexes).
   Exclusivity is enforced by the driver: it hands out a guard whose `Drop`
   releases the claim (so `Close` and process death both release it).
2. **`SetMode(rid, flags) -> 0`** (syscall 28): bit0 = non-blocking. Stored on the
   resource; resources that can block (keyboard, mouse; pipes in Stage 7) return
   `-7 WouldBlock` from `read` when non-blocking and empty. Resources without a
   blocking concept accept and ignore it. Default: blocking.
3. **Event wire formats** (libsci `devices` module, hand-rolled fixed-size encoding —
   *not* postcard, so streams can be framed by simple chunking):
   - `KeyEvent`: 2 bytes — `[key_code: u8, flags: u8]`, flags bit0 = pressed.
     Define `#[repr(u8)] pub enum KeyCode` in libsci covering the scancode set the
     kernel already parses (`devices/keyboard/scancode.rs`) **plus** F1–F12 and
     modifiers (Stage 11 needs F-keys for TTY switching; add scancodes now).
     Provide `KeyEvent::{to_bytes, from_bytes}`.
   - `MouseEvent`: 5 bytes — `[dx: i16 LE, dy: i16 LE, buttons: u8]` (bit0 L, bit1 R,
     bit2 M). Same to/from helpers.
   `read` on these devices returns only whole events (a `read` with `len < event
   size` → `InvalidInput`); a blocking read returns ≥1 event, up to as many whole
   events as fit.
4. **Keyboard resource:** rework the keyboard path: IRQ handler translates
   `Scancode` → `KeyEvent` and pushes into a `static KEY_EVENTS: Buffer<KeyEvent, 256>`.
   `KeyboardResource` pops (blocking `pop` / `try_pop` for non-blocking).
   **Delete the `keyboard_listener` kernel thread** (and its kmain registration) —
   scancodes stop being printed to the log. `get_info` →
   `ResourceInfo::Device { identifier: String }`.
5. **Framebuffer takeover (decided design):** new `FramebufferDriver`
   (`"vesa_fb"`, device `"fb0"`, `DeviceType::Framebuffer`).
   - `logger.rs` gains `detach_fb() -> Option<FbTerminal>` and
     `attach_fb(FbTerminal)`; the kernel log always writes serial + klog regardless.
   - On open: `detach_fb()`, extract the inner `Framebuffer`, build `FbResource`.
     On `Drop` of `FbResource`: rebuild an `FbTerminal` (cleared) and `attach_fb` —
     the kernel terminal returns when the owner closes or dies.
   - `FbResource`: `write(buf)` copies into the framebuffer at the current seek
     offset (clamped to buffer size; returns bytes written), `seek(offset)` sets it.
     A plain `write` of a full frame starting at seek 0 matches the doc's
     "writes from start of framebuffer each time" usage. `read` → `Unsupported`.
   - `get_info` → `ResourceInfo::Framebuffer { width, height, stride, bytes_per_pixel, pixel_format }`
     where `pixel_format` is a new libsci enum (`Rgb=0, Bgr=1, U8=2`) mapped from the
     bootloader's `PixelFormat`.
6. **PS/2 mouse driver** (`devices/mouse/`): init via the 8042 controller — enable
   aux port (0xA8), enable IRQ12 in the controller config byte, send 0xF4 (enable
   reporting) via 0xD4 prefix; add `PICInterrupt::Mouse = PIC_2_OFFSET + 4` and
   unmask IRQ12 (cascade IRQ2 must be unmasked too — check current PIC mask setup in
   `init_interrupts`). IRQ handler: 3-byte packet state machine (sync on bit3 of
   byte 0), decode to `MouseEvent` (sign bits from byte 0), push to
   `static MOUSE_EVENTS: Buffer<MouseEvent, 256>`. Driver `"ps2_mouse"`, device
   `"mouse"`, new `DeviceType::Mouse` (**append** to the enum). `MouseResource`
   mirrors the keyboard resource.
7. Register `FramebufferDriver` and `MouseDriver` in `init_devices`
   (`devices/manager.rs:82`).

## Work Items

1. libsci: `KeyCode`, `KeyEvent`, `MouseEvent`, `PixelFormat`, `DeviceType::Mouse`,
   `ResourceInfo::{Device, Framebuffer}` variants; syscalls 27–28 + docs; libsystem
   wrappers (`sys_open_device`, `sys_set_mode`) and an ergonomic
   `libsystem::devices::{Keyboard, Framebuffer, Mouse}` layer that decodes events /
   exposes `FbInfo`.
2. Keyboard rework: scancode→KeyCode translation table (extend `scancode.rs` for
   F-keys/modifiers), `KEY_EVENTS`, `KeyboardResource`, exclusivity guard, listener
   thread removal.
3. Framebuffer driver + logger detach/attach + `FbResource`.
4. Mouse driver (controller init, IRQ, decoder, resource).
5. `sys_open_device`, `sys_set_mode`; mode plumbing on resources.
6. ktest additions (input devices can't receive synthetic events yet, so tests cover
   the control plane; interactive verification is manual):
   - `lsdev_three_drivers`: `ListDevices` shows `ps2_keyboard`, `vesa_fb`,
     `ps2_mouse` with correct types.
   - `opendev_bogus`: `"nope:nope"` → `-10`; malformed id `"x"` → `-3` or `-10`
     (pick one, document).
   - `keyboard_exclusive`: open keyboard OK; second open → `-9`; close; reopen OK.
   - `keyboard_nonblock`: `SetMode(nonblock)`, `read` → `-7` (no human pressing
     keys in the test VM).
   - `fb_takeover_roundtrip`: open fb, `get_info` decodes with width/height > 0,
     write a 100×100 red square (compute offsets from stride/bpp), close. Then write
     a sentinel via the logger and read it back from klog — proves logging survived
     the takeover and the terminal reattached without panicking.
   - `fb_write_bounds`: seek past the end → write returns 0 bytes; huge write is
     clamped, no panic.
7. Manual verification (document results in the PR/commit message):
   - `cargo run`, watch adam (update adam to: open fb, fill blue, sleep 2 s, close —
     kernel terminal must come back and continue logging).
   - Type on the keyboard while a test program holds the keyboard device open and
     prints decoded `KeyEvent`s; verify keydown/keyup and F-keys.
   - Move the mouse in the QEMU window; verify sensible dx/dy/button events.

## Deliverables

- `OpenDevice`/`SetMode` syscalls; keyboard, framebuffer, mouse device resources
  with documented wire formats.
- Kernel-terminal handoff/reattach machinery.
- PS/2 mouse driver (new hardware support).

## Completion Criteria

1. `cargo run -- -test` passes all suites including the six new control-plane tests.
2. The three manual checks above behave as described (one-time, noted in the commit).
3. Normal boot log no longer prints raw scancodes (listener removed), and kernel
   logging is uninterrupted by fb open/close cycles.

## Out of Scope

- Sharing/multiplexing devices between processes (console does this in userspace,
  Stage 8/11).
- Synthetic input injection from the test runner (QMP `sendkey`) — only if Stage 8
  decides it needs it.
- USB HID, scroll wheel (Z-axis packets), scancode set 2 (roadmap).

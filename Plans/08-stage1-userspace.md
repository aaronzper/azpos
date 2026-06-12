# Stage 8 — Stage I Userspace: console, azpsh, info, syslogd — STAGE I COMPLETE

## Goal

The boot experience the design doc describes for Stage I: the machine boots, `adam`
(init) starts `syslogd` and `console`; the console owns the screen and keyboard,
spawns `azpsh`, renders its output, and feeds it typed input. A user at the keyboard
can run `info` and other programs. This stage closes out Stage I.

## Prerequisites

Stages 5 (files), 6 (keyboard/fb devices, `SetMode`), 7 (spawn/pipes/args).

## Current State

- All kernel facilities exist; userspace is just `adam` (demo) and the test
  programs. `libsystem` has raw wrappers plus the `fs`/`devices`/`process` helpers
  from prior stages.
- The kernel renders its own log to the framebuffer until someone opens the fb
  device (Stage 6 handoff).
- No userspace text rendering exists; the kernel's `FbTerminal`
  (`kernel/src/devices/fb/terminal.rs`) has the font/glyph logic to crib from.

## Design Decisions

1. **`libterm` crate** (`libraries/libterm`, `no_std`): reusable text-on-framebuffer
   renderer for userspace. Extract/duplicate the kernel `FbTerminal`'s font data and
   glyph blitting (check what font mechanism it uses — likely the `noto-sans-mono`
   bitmap or similar embedded data — and reuse the same approach; if the font data is
   a crate dependency, depend on it here too). API:
   `Term::new(fb_info) → Term`, `term.put_str(&str)`, scrolling, cursor, clear;
   renders into a caller-provided `Vec<u8>` back buffer; caller writes that buffer to
   the fb resource (seek 0 + write = full-frame present, per the doc's fb model).
   Keep it dumb: no ANSI escapes in Stage I (roadmap), but `\n`/`\r`/backspace.
2. **`console`** (`programs/console`): single TTY in Stage I (multi-TTY is
   Stage 11).
   - Setup: open `vesa_fb:fb0` + `ps2_keyboard:keyboard` (both `SetMode` non-block),
     read fb `get_info`, create two pipes (shell-stdin, shell-stdout), spawn
     `/programs/azpsh.exe` with `[stdin_end, stdout_end]` handles.
   - Main loop (poll-based, single-threaded — threads don't exist until Stage 12):
     1. drain keyboard events → translate `KeyEvent` to bytes (US layout table in
        console: letters/digits/symbols with shift, Enter→`\n`,
        Backspace→`0x08`) → write to shell-stdin pipe; **local echo is the
        shell's job, not the console's** (the console is a dumb byte mover).
     2. drain shell-stdout pipe → feed `libterm` → if anything changed, present the
        back buffer to the fb.
     3. nothing happened → `Sleep(5)`.
   - If the shell exits (stdout EOF + `wait_for_exit`), print
     `shell exited — respawning` and spawn a fresh one.
3. **`azpsh`** (`programs/azpsh`): reads stdin (handle 0, blocking is fine here),
   writes stdout (handle 1).
   - Line editor: echo printable bytes back to stdout as typed, handle backspace
     (emit `\x08 \x08`), Enter ends the line.
   - Parsing: whitespace-split; first token = command.
   - Builtins: `help`, `exit` (ends the shell → console respawns), `clear` (emit
     form-feed `0x0C`; libterm interprets as clear), `ls <path>` (ListDir → print),
     `cat <path>`, `rm <path>`, `mkdir <path>` — thin direct-syscall builtins so the
     shell is useful before more programs exist.
   - External commands: `foo a b` → `Spawn { path: "/programs/foo.exe", args: [a,b], handles: [own stdin, own stdout] }`
     (pipe handles are cloneable per Stage 7) → `wait_for_exit` → print exit
     notice. Unknown → `unknown command: foo`.
   - Prompt: `azpos> `.
4. **`info`** (`programs/info`): `Sysinfo` + `Time` → pretty-print OS name, version,
   uptime, memory, process count to stdout. Exit 0.
5. **`syslogd`** (`programs/syslogd`): open `Klog` resource; loop: read chunk → if
   nonempty, append to `/system/logs/kernel.log` (create dirs on first run:
   `MakeDir /system`, `/system/logs`, ignore `-11`); else `Sleep(250)`. Rotation:
   if the file exceeds 1 MiB, `Move` it to `kernel.log.old` (delete previous old)
   and start fresh — exercises Move/Delete in anger.
6. **`adam` becomes real init (Stage I form):** spawn `syslogd`, then `console`,
   each with no handles; then supervision loop: every 1000 ms, `ProcInfo` each
   child; if one died, log via logger resource and respawn it (max 3 respawns/min —
   simple counter, then give up on that child and keep going).
7. **build.rs:** add `console`, `azpsh`, `info`, `syslogd` artifact deps; write all
   into both images' `/Programs/`.
8. **Testing approach:** console/shell are interactive; automated coverage uses the
   pieces (pipes/spawn already covered). Add what *can* be automated:
   - ktest gains `shell_batch`: spawn `azpsh` with pipes, write
     `"info\nexit\n"`, read all output until EOF; assert it contains the prompt, a
     line from `info` (e.g. `azpOS`), and exits cleanly. (The shell is just a
     stdin/stdout program — fully testable headless!)
   - ktest gains `syslogd_files`: spawn syslogd, write a sentinel to the logger,
     sleep ~600 ms, kill syslogd, read `/system/logs/kernel.log`, assert the
     sentinel is present.
   - Console + libterm get a **manual checklist** (below).

## Work Items

1. `libterm` crate (+ extract font path from kernel terminal; document source).
2. `azpsh` (build it first — testable via pipes without any UI).
3. `info`.
4. `syslogd`.
5. `console` (incl. the keymap table; keep it a separate `keymap.rs` for Stage 11
   reuse).
6. `adam` init rewrite + build.rs wiring.
7. ktest: `shell_batch`, `syslogd_files`.
8. Manual checklist execution + record results.

## Deliverables

- Five real userspace programs + `libterm`.
- Booting `cargo run` lands in a usable shell on the framebuffer.
- Stage I of the design doc fully implemented.

## Completion Criteria

1. `cargo run -- -test` passes everything including `shell_batch` and
   `syslogd_files`.
2. Manual checklist (perform on `cargo run`, BIOS):
   - [ ] Boot lands in console (kernel messages stop; prompt appears).
   - [ ] Typing echoes; backspace works; Enter runs the line.
   - [ ] `info` prints system info; `help`, `ls /programs`, `cat`, `mkdir`, `rm`
         work; unknown commands report properly.
   - [ ] `exit` → console reports and respawns a fresh shell.
   - [ ] Reboot (`cargo run` again): `/system/logs/kernel.log` from the previous
         boot is still there (`cat` it) — syslogd + FAT persistence end-to-end.
   - [ ] 10 minutes idle at the shell: no panic, `info` still works (heap stability
         under the polling loops).
3. Check off stages 1–8 in `00-OVERVIEW.md`; tag the repo `stage-1-complete`
   (ask the user before tagging/pushing).

## Out of Scope

- Login, multi-TTY, fb sharing (Stage 11). ANSI escapes/colors in libterm
  (roadmap). Shell history/tab-completion (roadmap). Mouse use in console.
- Automated console-rendering tests (would need QMP screendump + image diffing —
  roadmap if console regressions become a problem).

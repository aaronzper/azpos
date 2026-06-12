# Stage 11 — Users & Permissions, Login, Multi-TTY, Framebuffer Sharing

## Goal

The Stage II security + console features: processes carry a user ID with kernel
enforcement (`set_user`, `set_owner`, guarded `kill`/`pinfo`/devices/ports), the
console gains login against the registry's `users` namespace, multiple virtual
TTYs, and framebuffer sharing so a child can "borrow" the screen.

## Prerequisites

Stages 8 (console), 10 (registry `users` namespace).

## Design Decisions

1. **Kernel user model (deliberately thin):** `Process.user: u32`, 0 = system/root.
   Admin-ness *in the kernel* is simply `uid == 0`; the registry's `admin` flag is
   userspace policy (console decides who may log in as whom / run what). Init runs
   as 0; `Spawn` inherits the parent's uid.
2. **Syscalls (34–35):**
   - `SetUser(uid) -> 0`: allowed iff current uid is 0 (else `-6`). One-way drop
     (root can also switch between non-zero uids — it stays root-only either way).
   - `SetOwner(rid, uid) -> 0`: resources gain `owner: u32` (defaults to creating
     process's uid). Owner-or-root may change it. Enforcement: `read`/`write`/
     `seek`/`close` on a resource you don't own… is impossible by construction
     (resources are per-process), so `SetOwner`'s real purpose is *handles passed
     to children* (Stage 7) and FB sharing: a resource whose owner isn't the
     process's current uid is read-only metadata-wise? **No — keep it honest:**
     enforcement is limited to `SetOwner` itself and `get_info` exposure
     (`ResourceInfo` gains `owner`); document that per-resource ACLs beyond this are
     not meaningful in a per-process handle model. This implements the doc's
     syscall without inventing semantics the design never specified.
   - **Spawn-as-user:** `SpawnRequest` gains `as_user: Option<u32>` (postcard
     append-compatible? **No** — struct field addition breaks postcard decoding of
     old encoders; since kernel & userspace ship together, bump freely but update
     all encoders in-tree; note this in the commit).
     Root-only (`-6` otherwise). This is how console starts a shell as the
     logged-in user.
3. **Kernel enforcement points** (all `-6` on violation):
   - `Kill`/`ProcInfo`: non-root may only target processes with its own uid
     (`ProcInfo` of others' procs: allowed read-only? **Decision: allowed** — `pinfo`
     is informational; only `Kill` is restricted).
   - `OpenDevice`: non-root denied (devices are the console's/root's business).
   - `IpcListen`: names with prefix `system.` require uid 0 (resolve the Stage 9
     TODO). `IpcConnect` unrestricted.
   - File syscalls: **no per-file permissions** (FAT stores none; documented
     limitation — the doc's permission model is satisfied at the process/device/
     port level).
4. **Registry schema additions:** `users/<uid>.username: Str`,
   `users/<uid>.admin: Bool`, optional `users/<uid>.password: Str` (**plaintext
   for now** — a no-dependency hash adds little real security in a toy single-user
   OS; flag prominently as a roadmap hardening item; if trivial, vendor a tiny
   pure-Rust SHA-256 and store `password_sha256` instead — implementer's choice,
   document which).
   registryd seed gains `users/0.password = Str("")` (empty = no password).
5. **Console: login + multi-TTY** (console stays root):
   - 4 virtual TTYs; each TTY = its own `libterm` back buffer + shell pipes + child
     shell PID + login state. F1–F4 switch (KeyCodes exist since Stage 6); only the
     active TTY's buffer is presented to the fb.
   - Login flow per TTY: prompt `login:` / `password:` (no echo for password),
     validate against registry via `libsystem::registry` (console is root, may
     read `users`); look up uid by username; on success
     `Spawn { path: azpsh, as_user: uid, handles: [pipes] }`; on shell exit →
     back to login prompt.
   - Status line (line 0, inverse video if libterm supports, else plain): TTY
     number + logged-in username.
6. **Framebuffer sharing** (doc: children "borrow" the framebuffer):
   - Mechanism: **spawn-time handle move.** Implement `try_clone` for `FbResource`
     as a *move-style* transfer: cloning an exclusive device is wrong, so instead
     `SpawnRequest.handles` handling gains per-handle semantics — new libsci type
     `HandleGrant { rid: ResourceID, mode: GrantMode }` where
     `GrantMode { Clone, Move }` (replaces the bare `Vec<ResourceID>`; same
     in-tree-ABI-bump caveat as above). `Move` removes the resource from the parent
     and gives the boxed resource itself to the child — works for *any* resource
     including exclusive devices, no clone needed.
   - Console flow: shell runs `fbdemo` → azpsh recognizes nothing special — instead
     **console policy**: a program list isn't needed; the shell asks the console.
     Simplest honest design: a console IPC port `console.fbshare` (root-gated names
     don't block this: console is root). `libsystem::fbshare::request() ->
     Connection`-based: a child asks the console for the fb; console moves its
     `FbResource` over… **handles can't move over IPC.** Final design, keep it
     concrete and simple:
     - azpsh builtin `fbrun <prog>`: shell sends the console a control message
       (over a dedicated pipe handle the console passes to every shell at spawn,
       handle 2: "console control channel"): `FBRUN <path>\n`.
     - Console pauses rendering, **closes its fb resource**, spawns `<path>` as the
       logged-in uid with **no fb handle at all** — the child opens the now-free
       device itself via `OpenDevice`… but non-root can't `OpenDevice` (rule 3).
       Resolve: console spawns the fb child as root? No. **Amend rule 3:**
       `OpenDevice` allowed for root, denied otherwise, *except* the kernel can't
       express "console blessed this". So: console opens the fb itself and passes
       it with `GrantMode::Move` in the child's handles (handle 3 by convention).
       Child draws via the moved handle; on child exit the resource drops →
       kernel terminal would reattach — **console must re-open the fb** when
       `wait_for_exit(child)` returns, then resume rendering. Brief kernel-terminal
       flash between drop and re-open is acceptable (note it).
   - Demo: `programs/fbdemo` — fills the screen with an animated gradient for ~3 s
     using `get_info` + seek/write, then exits.
7. **klog gating:** `Klog` syscall becomes root-only (`-6`) — log may contain
   sensitive info; syslogd runs as root (spawned by init). (Cheap, sensible,
   exercises a uid check on a misc syscall.)

## Work Items

1. Kernel: `Process.user`, inheritance, `SetUser`, `SetOwner` + resource owner
   field, enforcement points (Kill, OpenDevice, IpcListen `system.*`, Klog),
   `ProcessInfo.user` now real.
2. libsci/libsystem: syscalls 34–35, `SpawnRequest` v2 (`as_user`,
   `Vec<HandleGrant>`), `GrantMode::Move` kernel implementation (remove from
   parent table, insert into child).
3. registryd seed update + console login flow + uid lookup.
4. Console multi-TTY refactor (TTY struct, F-key switching, status line,
   per-TTY login). Shell control channel (handle 2) + `fbrun` builtin + console
   FBRUN handling.
5. `fbdemo` program; build.rs wiring.
6. ktest additions (ktest runs as uid 0 = init):
   - `setuser_drop`: spawn `ktest-child setuser-probe` as_user 1000; child:
     `SetUser(0)` → `-6`; `GetPid`/`ProcInfo(self)` shows uid 1000; child tries
     `OpenDevice(keyboard)` → `-6`; `Klog` → `-6`; `IpcListen("system.x")` → `-6`;
     `IpcListen("user.x")` → ok; exit 0.
   - `kill_cross_user_denied`: spawn `hang` as uid 1000 and a killer child as uid
     2000 that tries to kill it → child reports `-6`; root ktest then kills both.
   - `spawn_as_user_root_only`: a uid-1000 child trying `as_user` spawn → `-6`.
   - `handle_move`: spawn echo child with a pipe end passed `GrantMode::Move` —
     parent's RID is gone (`-1` on use), child uses it fine.
   - `set_owner`: create pipe, `SetOwner(rid, 42)` as root → `get_info` shows 42;
     as non-root child on a granted handle → `-6`.
7. Manual checklist: login as root on tty1; F2 → second login; run `fbrun fbdemo`
   → gradient; on exit console returns intact; wrong password rejected.

## Deliverables

- uid model + enforcement; `SetUser`/`SetOwner`; spawn-as-user + handle
  move-grants; login + 4 TTYs; fb sharing with `fbdemo`.

## Completion Criteria

1. `cargo run -- -test` passes; the five new permission tests prove each
   enforcement point.
2. Manual checklist passes (record in commit message).
3. Grep audit: every syscall handler that should check uid does (list them in the
   PR description against design item 3).

## Out of Scope

- File ownership/permissions (FAT limitation — revisit with ext2 on the roadmap).
- Password hashing decision is implementer's-choice/documented; real credential
  storage is roadmap.
- Handle transfer over IPC (roadmap; would supersede the FBRUN dance).

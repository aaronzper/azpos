# Stage 10 — System Registry: registryd + client API

## Goal

The Stage II System Registry: a namespaced, typed key-value store persisted under
`/system/registry`, owned by the `registryd` daemon, accessed by all other programs
over IPC through `libsystem`. Seeds the `sys` and `users` namespaces the design doc
specifies (Stage 11 consumes `users`).

## Prerequisites

Stages 5 (file write), 9 (IPC + framing helpers).

## Current State

- IPC with length-prefixed message helpers exists; VFS write/persistence is tested.
- `adam` supervises `syslogd` + `console` (Stage 8).

## Design Decisions

1. **Data model** (new libsci module `registry`, postcard, append-only):
   ```rust
   pub enum RegValue { Str(String)=…, Bool(bool), U64(u64), I64(i64), Bytes(Vec<u8>) }
   ```
   Keys: `[a-z0-9_.]{1,64}`. Namespaces: same charset. Dotted *key* paths inside a
   namespace are allowed and purely conventional (e.g. users ns, key `0.username`)
   — the doc's nested `users.[id].username` schema maps to namespace `users`, key
   `"<id>.username"`. This keeps the store flat and simple.
2. **On-disk format** (per the doc's layout):
   ```
   /system/registry/<namespace>/entries.reg    postcard Vec<(String, RegValue)>
   /system/registry/<namespace>/metadata.reg   postcard NamespaceMeta { owner_uid: u32, world_writable: bool }
   ```
   `registryd` loads everything at startup, holds it in a `BTreeMap`, and
   write-through saves a namespace's `entries.reg` on every mutation (write to
   `entries.reg.tmp`, then `Delete` old + `Move` tmp into place — VFS has no atomic
   rename-over, so crash windows exist; acceptable, noted).
3. **Protocol** over IPC port **`system.registry`** (libsci `registry` module, used
   by both sides; one request → one response per message, via Stage 9
   `send_msg`/`recv_msg`):
   ```rust
   pub enum RegRequest {
       Get { ns: String, key: String },
       Set { ns: String, key: String, value: RegValue },
       Delete { ns: String, key: String },
       ListKeys { ns: String },
       ListNamespaces,
       CreateNamespace { ns: String },
   }
   pub enum RegResponse {
       Value(RegValue), Ok, Keys(Vec<String>), Namespaces(Vec<String>),
       Err(RegError),  // enum: NotFound, BadName, NotPermitted, Io
   }
   ```
   Permissions are accepted-but-unenforced in this stage (`metadata.reg` is written;
   enforcement lands with real uids in Stage 11 — `// TODO(stage-11)` at the Set
   path).
4. **`registryd`** (`programs/registryd`): single-threaded accept loop — `IpcBind`
   blocking; per connection, handle messages until EOF, then accept the next.
   (Serving one client at a time is fine: transactions are single-message;
   `libsystem`'s client connects per-operation. Persistent connections + threads are
   a Stage 12 upgrade if profiling ever cares.) First run: if
   `/system/registry` is missing, create it and seed defaults:
   `sys/hostname = Str("azpos")`, `users/0.username = Str("root")`,
   `users/0.admin = Bool(true)`.
5. **Client API** (`libsystem::registry`): `get(ns, key)`, `set(ns, key, value)`,
   `delete`, `list_keys`, `list_namespaces`, `create_namespace` — each does
   connect → send → recv → close, with a bounded retry-with-`Sleep` while
   registryd isn't up yet (daemon start ordering).
6. **`adam`**: spawn order becomes `registryd` → `syslogd` → `console`, same
   supervision rules. (The doc calls Stage II init "adamd"; the binary stays
   `/Programs/init.exe` built from the `adam` crate — rename the crate only if it
   bothers you later; not load-bearing.)

## Work Items

1. libsci `registry` module (`RegValue`, requests/responses, name-validation
   helpers shared by client and daemon).
2. `programs/registryd` (load, serve, persist, seed).
3. `libsystem::registry` client.
4. `adam` ordering + build.rs wiring (both images).
5. ktest additions (registryd is spawned by ktest itself in the test image — init
   there is ktest, not adam):
   - `reg_seed`: spawn registryd; `get("sys","hostname")` → `"azpos"`;
     `users/0.username` → `"root"`.
   - `reg_set_get_types`: set/get round-trip one value of each `RegValue` variant
     in a fresh namespace.
   - `reg_persistence`: set `test/marker`, kill registryd, respawn, get →
     value survives (disk round-trip).
   - `reg_list_delete`: ListKeys shows what was set; Delete removes; Get → NotFound.
   - `reg_bad_names`: `ns "BAD NAME!"` → BadName error response (and the daemon
     survives malformed postcard bytes sent raw via send_msg — returns Err or
     closes the connection, but doesn't die: send garbage, then do a valid Get).
   - `reg_concurrent_clients`: 2 spawned `ktest-child reg-hammer` children each do
     200 set/get cycles in distinct namespaces; both exit 0 (serialization
     correctness, not perf).
6. Manual: boot `cargo run`, in azpsh run a new builtin `reg get sys hostname`
   (add `reg get/set` builtins to azpsh as part of this stage — first real consumer
   of the client API and handy forever).

## Deliverables

- `registryd`, wire protocol in libsci, client in libsystem, seeded schema,
  on-disk persistence, azpsh `reg` builtin.

## Completion Criteria

1. `cargo run -- -test` passes all `reg_*` tests including persistence-across-
   daemon-restart and the garbage-input survival test.
2. Manual: `reg get sys hostname` at the shell prints `azpos`;
   `reg set sys hostname box1` then reboot, `reg get` → `box1` (full-system
   persistence).

## Out of Scope

- Permission *enforcement* (Stage 11), watches/subscriptions, transactions,
  multi-client concurrency in registryd (Stage 12 threads would enable it;
  roadmap).

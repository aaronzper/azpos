# Stage 9 — IPC: Named Ports (ipc_listen / ipc_bind / ipc_connect)

## Goal

Local-socket IPC per the design doc: a server process listens on a named port,
clients connect, both sides get bidirectional stream resources. This begins
Stage II and is the transport for the System Registry (Stage 10).

## Prerequisites

Stage 7 (pipe machinery — connections reuse `PipeBuffer`).

## Current State

- `PipeBuffer`/`PipeResource` (Stage 7) implement blocking/non-blocking ring-buffer
  streams with EOF/Closed semantics and condvar wakeups.
- No process-to-process rendezvous mechanism exists (pipes require a common
  ancestor to pass handles).

## Design Decisions

1. **Ports are strings** (overview decision): UTF-8, 1–64 bytes, convention
   dot-namespaced (`"system.registry"`). Global table in a new
   `kernel/src/processes/ipc.rs`:
   `static PORTS: KMutex<BTreeMap<String, Arc<Listener>>>`.
   `Listener { pending: KMutex<VecDeque<Box<PipeResource-like server half>>>, arrived: KCondvar, closed: AtomicBool }`.
2. **Syscalls** (31–33):
   - `IpcListen(name_ptr, name_len) -> rid`: registers the name → `ListenerResource`
     (`ResourceType::IpcListener`). `-11` if the name is taken. The name is freed
     when the listener resource drops (close/exit) — pending unaccepted connections
     get their halves marked closed.
   - `IpcConnect(name_ptr, name_len) -> rid`: `-10` if nobody is listening (no
     blocking-until-server-appears; clients retry — document the retry-with-Sleep
     pattern in libsystem). Otherwise: build a connection = two `PipeBuffer`s; the
     client half is returned immediately as an `IpcConnection` resource
     (`ResourceType::IpcConnection`, stream semantics identical to a pipe end —
     writes buffer up to 4096 even before the server binds); the server half is
     queued on the listener + condvar notify. Pending queue cap: 16; beyond that
     `IpcConnect` returns `-9 Busy`.
   - `IpcBind(listener_rid) -> rid`: blocks until a pending connection exists, pops
     it, returns the server-half `IpcConnection` RID. Non-blocking mode (`SetMode`)
     → `-7` when none pending. Returns `-8` if the listener was closed out from
     under a blocked bind (can't happen single-threaded; matters in Stage 12).
   - `read`/`write`/`get_info` on `IpcConnection` = pipe semantics
     (`ResourceInfo::IpcConnection { buffered, peer_closed }`).
3. **No permissions on ports** in this stage (any process may listen/connect);
   Stage 11 may reserve the `system.*` prefix for uid 0 — leave a `// TODO(stage-11)`
   at the listen path.
4. **Framing is userspace's problem:** connections are byte streams. `libsystem`
   gains `ipc` module: `Listener::listen(name)`, `Listener::accept()`,
   `Connection::connect(name)`, plus **length-prefixed message helpers**
   `send_msg(&[u8])` / `recv_msg() -> Vec<u8>` (u32 LE length prefix, 1 MiB sanity
   cap) — the registry protocol (Stage 10) and any future daemon use these instead
   of inventing framing each time.
5. Cleanup invariants (write tests for each): listener drop frees the name for
   re-listen; process exit drops listeners and connections (via the Stage 4
   resource-table drop) and peers observe EOF/`-8`; a connection whose peer died
   mid-stream drains buffered bytes then EOFs.

## Work Items

1. `ipc.rs` (ports table, `Listener`, connection construction reusing
   `PipeBuffer`), `ListenerResource`, `IpcConnection` resource.
2. Syscalls 31–33 + libsci docs/types (`ResourceType`/`ResourceInfo` variants) +
   libsystem `ipc` module with framing helpers.
3. `terminate_proc` audit: resource drops already handle cleanup; verify with the
   tests rather than new code.
4. `ktest-child` new modes:
   - `ipc-server <port>`: listen, accept one connection, echo messages
     (recv_msg/send_msg uppercased) until EOF, exit.
   - `ipc-flood <port>`: connect 20 times in a loop without the server binding.
5. ktest additions:
   - `ipc_basic`: spawn `ipc-server test.echo`; connect (retry loop ≤ 2 s);
     send_msg "hello" → recv "HELLO"; close; child exits.
   - `ipc_no_listener`: connect to `nobody.home` → `-10`.
   - `ipc_name_conflict`: listen `test.dup` twice (second listener in-process via a
     second `IpcListen` call) → `-11`; close first; re-listen succeeds.
   - `ipc_backlog`: spawn `ipc-flood` against an in-ktest listener that never binds
     → child observes `-9` after 16; then ktest binds 16 times and each connection
     EOFs or echoes correctly.
   - `ipc_server_death`: spawn server, connect, kill server, `read` drains then
     EOFs, `write` → `-8`.
   - `ipc_early_write`: connect, send_msg *before* server binds, server binds and
     receives it (buffering works).

## Deliverables

- Named-port IPC syscalls + kernel objects, with documented stream + framing
  conventions and a libsystem client/server API.

## Completion Criteria

1. `cargo run -- -test` passes all six new IPC tests plus the full prior suite.
2. `ipc_server_death` specifically passes 10 iterations in a loop (kernel object
   cleanup under repeated churn — no leak: assert `Sysinfo.heap_used_bytes` doesn't
   grow monotonically across iterations).

## Out of Scope

- Datagram/message-mode sockets, scatter-gather, handle transfer over IPC
  (roadmap — note: handle transfer would be the clean way to do fb sharing, but
  Stage 11 uses spawn-time handle passing instead).
- Port discovery/enumeration syscall (roadmap; the registry serves this need at the
  application level).

# Stage 5 — VFS, FAT32 Write Support, File Syscalls

## Goal

A global virtual filesystem with mount points, FAT32 read **and write** support, and
the file syscall set: `OpenFile`, `ListDir`, `MakeDir`, `Move`, `Delete`, `LockDir`,
`UnlockDir`. After this stage, the disk is a live, persistent store available to any
process for the rest of uptime — and `spawn` (Stage 7) and the registry (Stage 10)
have what they need.

## Prerequisites

Stages 2–4. (Stage 4's `terminate_proc` is where directory-lock cleanup hooks in.)

## Current State

- `FileSystem<'a>` trait (`kernel/src/filesystem/mod.rs`) **borrows** its
  `BlockDevice` (`mount(drive: &'a mut dyn BlockDevice)`); read-only
  (`dir_contents`, `read_all`).
- FAT32 (`filesystem/fat/`): boot record, FAT chain reading, directory traversal.
  Name lookup uppercases the search name (`get_directory`) — **verify LFN support**;
  if `directories.rs` only parses 8.3 short names, LFN parsing is part of this stage
  (the design doc requires "FAT32 + LFN"). `adam.exe` fits in 8.3, so this has never
  been exercised.
- The only mount happens inside a throwaway thread closure in `kmain`
  (`kernel/src/main.rs:66-84`): PCI scan → AHCI → `partition()[2]` → mount → read
  init ELF → everything dropped. **The disk is unreachable after boot.**
- AHCI `BlockDevice` supports `read_blocks` and `write_blocks` already
  (`devices/storage/mod.rs`); `partition(self)` consumes the device and yields owned
  partition `BlockDevice`s.
- `FilePath` exists (`filesystem/path.rs`).

## Design Decisions

1. **Ownership refactor:** `FileSystem` loses its lifetime:
   ```rust
   pub trait FileSystem: Send {
       fn mount(drive: Box<dyn BlockDevice + Send>) -> FileSystemResult<Self> where Self: Sized;
       fn unmount(self: Box<Self>) -> Box<dyn BlockDevice + Send>;
       // reads
       fn dir_contents(&mut self, path: &FilePath) -> FsResult<Box<[FileMetadata]>>;
       fn file_size(&mut self, path: &FilePath) -> FsResult<u64>;
       fn read_at(&mut self, path: &FilePath, offset: u64, buf: &mut [u8]) -> FsResult<usize>;
       // writes
       fn create_file(&mut self, path: &FilePath) -> FsResult<()>;
       fn write_at(&mut self, path: &FilePath, offset: u64, data: &[u8]) -> FsResult<usize>; // extends file as needed
       fn truncate(&mut self, path: &FilePath, len: u64) -> FsResult<()>;
       fn make_dir(&mut self, path: &FilePath) -> FsResult<()>;
       fn rename(&mut self, from: &FilePath, to: &FilePath) -> FsResult<()>;
       fn delete(&mut self, path: &FilePath) -> FsResult<()>; // file or *empty* dir
   }
   ```
   Path-based (FAT has no inodes); `FileResource` carries a path + position. Add
   `BlockDevice: Send` bounds where needed (`unsafe impl Send` for the AHCI device
   with a comment justifying it: single-threaded access is enforced by the VFS lock).
   `FileMetadata` gains `size: u64`.
2. **VFS:** new `filesystem/vfs.rs` with
   `pub static VFS: KMutex<Option<Vfs>>`. `Vfs` holds a mount table
   `Vec<(FilePath, Box<dyn FileSystem>)>`; `resolve(&FilePath) -> (&mut dyn FileSystem, FilePath)`
   by longest-prefix match. Only mount at boot: the FAT32 root partition at `/`.
   Mounting more filesystems is kernel-internal API (no mount syscall — not in the
   design doc). **One big VFS lock**: all file syscalls serialize through
   `VFS.lock()`, including disk I/O. Acceptable for now; flagged on the roadmap.
3. **Boot refactor:** replace the kmain closure with
   `filesystem::init_root_fs()` — kernel thread that does PCI→AHCI→partition 2→
   `FATFilesystem::mount`→install into `VFS`→`spawn_proc` init by reading
   `/programs/init.exe` *through the VFS*. (Keep partition index 2; add a comment
   explaining it's the data partition the build script creates after the
   bootloader's.)
4. **FAT32 write internals** (in order): free-cluster scan + allocation (update FSInfo
   if present, else just FAT), cluster-chain extend/truncate/free, directory-entry
   creation with 8.3 short-name generation + LFN entry chains (and checksum),
   directory growth (allocate new cluster when a dir is full), file-size updates,
   entry removal (0xE5 marking), rename = new entry + remove old (same FS only).
   **Write-through**: every metadata update hits the disk before the call returns
   (no dirty cache — slow but correct; caching is roadmap).
5. **`FileResource`** (kernel, `ResourceType::File`): `{ path: FilePath, pos: u64, readable: bool, writable: bool, append: bool }`.
   `read`/`write` lock the VFS per call and use `read_at`/`write_at`; `seek` sets
   `pos`; `get_info` → `ResourceInfo::File { size, pos, path: String }`.
6. **Syscall ABIs** (all paths are `(ptr, len)` UTF-8 absolute paths via Stage 2
   `user_str`; relative paths → `InvalidInput` — there is no CWD concept):
   - `OpenFile(path_ptr, path_len, flags) -> rid`. Flags bitfield: 1=read, 2=write,
     4=create (create if missing), 8=truncate, 16=append. Opening a directory →
     `InvalidInput`. Missing without create → `-10`.
   - `ListDir(path_ptr, path_len) -> rid` — returns a `BlobResource` containing
     postcard `Vec<DirEntry>`; new libsci type
     `DirEntry { name: String, is_dir: bool, size: u64 }`. (Blob pattern, like
     `ListDevices`.)
   - `MakeDir(path_ptr, path_len) -> 0` (`-11` if exists, `-10` if parent missing).
   - `Move(rid, path_ptr, path_len) -> 0` — per the design doc, move operates on an
     **open file resource**: renames the underlying path; the resource's stored path
     is updated. Cross-filesystem moves → `Unsupported` (until a second FS exists).
   - `Delete(path_ptr, path_len) -> 0` — file or empty dir; `-10` missing; non-empty
     dir → `InvalidInput`. (Deliberate extension beyond the doc — see overview.)
   - `LockDir(path_ptr, path_len) -> 0` / `UnlockDir -> 0`: VFS keeps
     `locks: Vec<(FilePath, ProcessID)>`. While locked by P, any *other* process's
     `OpenFile`/`MakeDir`/`Move`/`Delete`/`LockDir` whose target resolves **inside**
     that directory (any depth) fails with `-12`. `ListDir` is allowed. Locks are
     released by `UnlockDir` (owner only) and in `terminate_proc` (add the hook).
7. **Open-file semantics with concurrent mutation:** no open-file table dedup; two
   opens of the same path are independent resources. `Delete`/`Move` of a file
   another resource has open is *allowed* and subsequent ops on the stale resource
   return `-10`. Documented in libsci rustdoc; proper coordination is roadmap.

## Work Items

1. Trait refactor + AHCI/`mbr` Send plumbing + FAT impl signature updates (reads
   first, keep ktest green via the boot path).
2. LFN audit/implementation in `directories.rs` (read side), then write side.
3. FAT write internals (design item 4), each with a focused ktest case.
4. `vfs.rs`, `init_root_fs()`, kmain cleanup.
5. `FileResource` + syscalls 20–26 + libsci docs/types + libsystem wrappers
   (`sys_open_file`, … and an ergonomic `libsystem::fs::File::{open, create}` thin
   wrapper struct with `read_to_end`).
6. Directory-lock table + `terminate_proc` hook.
7. ktest additions (run order matters; use `/test-data/` as a scratch dir created by
   the test itself):
   - `mkdir_lsdir`: `MakeDir /test-data`, `ListDir /` contains it with `is_dir`.
   - `create_write_read`: create `/test-data/a.txt`, write 100 bytes pattern, close,
     reopen read, `get_info` size==100, content matches.
   - `append_seek`: append 50 more, seek 75, read 50, verify spans the boundary
     (crosses a cluster if pattern sized right — use 5000 bytes to span clusters).
   - `large_file`: write 200 KiB (multi-cluster chain), read back, verify (every
     64th byte).
   - `lfn_names`: create `Long File Name test.txt` (LFN + spaces + case), `ListDir`
     shows the exact name, reopen by name works.
   - `move_open_file`: open, `Move` to `/test-data/renamed.txt`, write more, close;
     reopen by new name has all bytes; old name `-10`.
   - `delete`: delete file → `ListDir` no longer shows it; delete non-empty dir →
     `-3`; delete missing → `-10`.
   - `lockdir_self`: lock `/test-data`, own opens still work, `UnlockDir`, double
     unlock → error. (Cross-process lock test arrives in Stage 7.)
   - `persistence`: **two-boot test.** The runner gains a `-test2` second phase? No —
     simpler: ktest writes `/test-data/persist.txt` then `Power(1)` (reboot) *only
     if the file didn't exist at startup*; on the second boot it verifies content and
     continues the full suite. The runner allows exactly one reboot in test mode
     (track a reboot count; two reboots = fail). This proves writes hit the disk.
   - `read_all_programs`: `ListDir /programs` shows `init.exe` and `adam.exe`;
     reading `adam.exe` returns a valid ELF magic. (Guards the Stage 7 spawn path.)

## Deliverables

- Owned-device `FileSystem` trait; global VFS mounted at `/` for the whole uptime.
- FAT32: LFN + full write support, write-through.
- Syscalls 20–26 + libsystem `fs` module.
- Boot path reads init through the VFS.

## Completion Criteria

1. `cargo run -- -test` passes the full suite **including the reboot-persistence
   test** (runner exits 0 after the second boot's clean shutdown).
2. After a test run, mount `test.img`'s FAT partition on the host (e.g.
   `hdiutil attach` / `mtools`) and confirm `/test-data/persist.txt` exists with the
   right content — validates on-disk format compatibility with a real FAT
   implementation. Do the same check for an LFN file. (Manual, once.)
3. Normal `cargo run` still boots adam via the VFS path.

## Out of Scope

- Second filesystem (ext2 — roadmap), mount/umount syscalls, CWD/relative paths.
- Block/page caching, async I/O, finer-grained VFS locking (roadmap).
- File permissions/owners (FAT can't store them; Stage 11 notes this).

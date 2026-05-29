# Community Boundary Refactoring Plan

Graphify identified communities `0`, `1`, `2`, and `3` as low-cohesion because several files currently mix dispatch, domain logic, and orchestration. This document defines the intended module boundaries so future changes move code toward tighter communities instead of adding more cross-cutting edges.

## Community 0 — Linux syscall handlers

**Original hot spot:** `src/abi/linux/mod.rs`

**Boundary:** Linux ABI facade + syscall family modules.

Status: the first split is applied. `src/abi/linux/mod.rs` is now a thin facade that delegates syscall routing into focused family dispatch modules (`fs`, `process`, `memory`, `time`, `net`, `misc`). Handler bodies can be migrated behind those dispatch boundaries incrementally.

Target layout:

```text
src/abi/linux/
  mod.rs              # LinuxAbiPlugin facade, syscall dispatch table only
  nr.rs               # Linux x86_64 syscall numbers
  types.rs            # Linux ABI structs/constants shared by handlers
  fs.rs               # open/read/write/stat/getdents/chdir/etc.
  process.rs          # exit/clone/wait/getpid/kill/tgkill/etc.
  memory.rs           # brk/mmap/mprotect/munmap/madvise/etc.
  time.rs             # nanosleep/clock_gettime/gettimeofday/etc.
  net.rs              # socket/bind/listen/connect/send/recv/etc.
  signal.rs           # rt_sigaction/rt_sigprocmask/signal fd stubs
  misc.rs             # uname/ioctl/fcntl/prctl/sysinfo/etc.
  elf_loader.rs       # ELF loading only
```

Rules:
- `mod.rs` must stay a facade: plugin metadata, load delegation, and high-level family dispatch only.
- Family modules may depend on `SyscallContext`, process, VFS, memory, and net as needed.
- Handler bodies should move into their owning family modules over time.
- Syscall-number constants should move to `nr.rs` next.
- Linux structs and error constants should live in `types.rs` only.

## Community 1 — ZiqaFS implementation

**Current hot spot:** `src/fs/ziqafs.rs`

**Boundary:** on-disk format, allocation, journal, directory, and VFS adapter are separate concerns.

Target layout:

```text
src/fs/ziqafs/
  mod.rs          # ZiqaFs facade and public re-exports
  layout.rs       # Superblock, Inode, constants, serialization
  bitmap.rs       # block/inode bitmap operations
  journal.rs      # JournalHeader, JournalEntry, replay/commit
  dir.rs          # directory entry encode/decode/enumeration
  file.rs         # ZiqaFsFile implementation
  fsck.rs         # FsckResult and consistency checks
  mount.rs        # mount_into_vfs and formatting/mount glue
```

Rules:
- Only `layout.rs` knows byte offsets and disk-layout constants.
- Only `bitmap.rs` mutates allocation bitmaps.
- Only `journal.rs` writes journal records.
- VFS-facing code stays in `file.rs`/`mount.rs`, not in allocator/layout code.

## Community 2 — Interactive shell and utilities

**Current hot spot:** `src/shell.rs`

**Boundary:** shell core, line editing, command registry, and command families.

Target layout:

```text
src/shell/
  mod.rs             # Shell struct and event loop
  line_editor.rs     # history, cursor, autocomplete, read_line
  commands/mod.rs    # Command enum/dispatch table
  commands/fs.rs     # ls/cd/cat/mkdir/rm/mv/cp/touch/stat/du
  commands/proc.rs   # ps/spawn/exec/kill/sleep
  commands/net.rs    # ping/wget/ifconfig/netstat
  commands/system.rs # help/uptime/meminfo/diskinfo/klog/reboot/clear/echo
  commands/demo.rs   # doom/tetris
```

Rules:
- `Shell::run` should parse input and dispatch; command bodies belong in command modules.
- Command modules receive a small shell context instead of accessing mutable shell internals directly.
- DNS helpers stay in `net`, not in shell.

## Community 3 — startup, syscall dispatcher, heap init cross-cluster

**Current hot spots:** `src/lib.rs`, `src/main.rs`, `src/abi/syscall.rs`, `src/memory/heap.rs`, `src/fs/ramfs.rs`, `src/arch/x86_64/interrupts.rs`

**Boundary:** boot orchestration is separate from runtime demos and subsystem implementations.

Applied now:
- `src/init.rs` owns early kernel initialization and ABI registry setup.
- `src/lib.rs` is now a crate facade/re-export surface plus global boot-info storage.

Target layout:

```text
src/init.rs             # early boot init only
src/main.rs             # boot presentation and runtime demo orchestration only
src/abi/syscall.rs      # generic syscall context + dispatch only
src/memory/heap.rs      # heap setup only
src/arch/x86_64/        # interrupts and CPU-specific traps only
src/fs/ramfs.rs         # in-memory file implementation only
```

Rules:
- `main.rs` may orchestrate demos but must not implement subsystem internals.
- `abi/syscall.rs` must not know Linux-specific syscall numbers; ABI plugins own those.
- Interrupt handlers should delegate to subsystem APIs instead of embedding policy.

## Refactoring order

1. Keep `src/init.rs` boundary in place.
2. Extract Linux syscall numbers into `src/abi/linux/nr.rs`.
3. Move Linux syscall bodies by family, starting with `memory.rs` and `process.rs` because they have the clearest dependencies.
4. Split shell command bodies after introducing a command context type.
5. Split ZiqaFS last; preserve on-disk layout tests/checks while moving layout and bitmap code first.

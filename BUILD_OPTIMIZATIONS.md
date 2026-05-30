# ZiqaKernel Build Optimizations

## Build Speed Improvements

### 1. Cargo Configuration (.cargo/config.toml)
- `codegen-units = 16` - Parallel code generation across crates
- `incremental = true` - Incremental compilation cache
- `opt-level = 1` for dev, `opt-level = 0` for dependencies - Faster debug builds
- `jobs = 8` - Parallel compilation jobs

### 2. Profile Configuration (Cargo.toml)
- `edition = "2021"` - Stable edition
- `[profile.dev.package.*] opt-level = 0` - Dependencies compiled with no optimization
- `[profile.dev] codegen-units = 16` - More parallel codegen units

### 3. Makefile for Quick Builds
- `make fast` - Parallel build with max jobs
- `make inc` - Incremental rebuild
- `make build` - Standard debug build

## Build Times
- Clean build: ~60-120 seconds
- Incremental build: ~1.7-2 seconds
- Release build: ~60 seconds

## Kernel Enhancements Added

### Core Subsystems
1. **VFS (Virtual File System)** - Capability-based file operations
2. **RamFS** - In-memory filesystem
3. **Page Cache** - LRU page cache with 64KB capacity
4. **IPC** - Inter-process communication channels with ring buffers
5. **eBPF** - Extended Berkeley Packet Filter with verifier and VM
6. **io_uring** - Asynchronous I/O ring interface
7. **MLFQ Scheduler** - Multi-level feedback queue scheduler
8. **Keyboard Buffer** - Input ring buffer for stdin
9. **Heap Profiler** - Memory allocation tracking
10. **Performance Suite** - Benchmarking utilities

### FAT32 Development Disk
ZiqaKernel now supports mounting a host-editable FAT32 partition at `/fat` when the boot disk contains an MBR FAT32 partition. Create one with:

```bash
make fat-disk
make run
```

Notes:
- The active in-kernel FAT32 path is a no_std read-only VFS bridge in `src/fs/fat32.rs`.
- `fatfs` is tracked as an optional upstream crate candidate, but its current no_std path depends on the old `core_io` crate, which is not compatible with this nightly toolchain. We keep the architecture ready for it while using the kernel-native driver for now.

### Build Configuration
```toml
[profile.dev]
opt-level = 1
panic = "abort"
codegen-units = 16
incremental = true

[profile.dev.package.*]
opt-level = 0
```

## Future Optimizations
- Install sccache: `cargo install sccache`
- Use `cargo install cargo-cache`
- Enable distributed caching for team builds

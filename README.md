# ZiqaKernel Engineering Report
<div align="center">
  <img src="assets/logo.svg" alt="ZiqaKernel Logo" width="250"/>
  <h1>ZiqaKernel</h1>
  <p><strong>OS Research Playground & Architecture Lab</strong></p>
</div>

<p align="center">
  <img src="https://img.shields.io/badge/rust-nightly-orange?logo=rust" alt="Rust Nightly"/>
  <img src="https://img.shields.io/badge/zig-★-gold?logo=zig" alt="Zig"/>
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="License"/>
  <img src="https://img.shields.io/badge/arch-x86__64-purple" alt="x86_64"/>
  <img src="https://img.shields.io/badge/status-experimental-yellow" alt="Status"/>
  <img src="https://img.shields.io/badge/version-0.1--dev-blueviolet" alt="Version 0.1"/>
  <img src="https://img.shields.io/badge/documentation-graph-brightgreen" alt="Knowledge Graph"/>
  <img src="https://img.shields.io/badge/Maintained%3F-yes-green.svg" alt="Maintenance"/>
  <img src="https://img.shields.io/badge/graph_benchmark-75x-brightgreen" alt="75x Token Reduction"/>
</p>

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 🔬 Executive Summary
ZiqaKernel is an **experimental OS research sandbox** written in Rust for `x86_64` bare metal — with select hot paths in **Zig**. It acts as a testbed for advanced OS design patterns: **SMP + APIC**, **ACPI/PCI enumeration**, **Instant Capability Revocation**, **A-Tier Scalable Architecture**, **Plugin-based ABI Layer**, **Capability-based Security**, **Hybrid Rust/Zig FFI**, **eBPF "Obsidian-Tier" VM (Maps/Helpers/Stack/Tail Calls/Tracepoints)**, **io_uring**, **Per-Process Page Tables + COW Fork**, **VMA-based Memory Management**, **PS/2 Mouse**, **FAT32 (read-only)**, **Userspace Drivers (DRM/Net)**, **TCP/UDP Socket Stack**, **NWCC Desktop Demo (6 apps)**, **DOOM fire / Tetris demos**, and a staged VGA boot experience.

**Key Architectural Insights (May 2026):**
- **SMP + APIC (Complete)**: Symmetric Multi-Processing with full AP boot via INIT-SIPI-SIPI protocol (16-bit → 64-bit trampoline), per-CPU structs via GS.base MSR (64-CPU support), Local APIC + I/O APIC interrupt management, IPI delivery (reschedule, TLB shootdown), and PIT-calibrated APIC timer. See [`src/arch/x86_64/smp.rs`](src/arch/x86_64/smp.rs) and [`src/arch/x86_64/apic.rs`](src/arch/x86_64/apic.rs).
- **ACPI + PCI Enumeration (Complete)**: Full ACPI table parsing (RSDP/MADT/FADT via the `acpi` crate) for processor topology and interrupt routing. Legacy PCI bus scan (CF8/CFC), BAR decoding, and class-based device discovery. [`src/drivers/acpi.rs`](src/drivers/acpi.rs), [`src/drivers/pci.rs`](src/drivers/pci.rs).
- **Device Driver Model (Complete)**: Generic `Driver` trait with PCI match/init lifecycle and a global `DeviceManager` that auto-probes discovered hardware. Block devices registered by name in a shared `BlockRegistry`. [`src/drivers/device_manager.rs`](src/drivers/device_manager.rs), [`src/drivers/block_registry.rs`](src/drivers/block_registry.rs).
- **Instant Capability Revocation (A+ Tier)**: Implemented a recursive **Revocation Tree**. Parents can instantly "pull the plug" on delegated capabilities, severing access system-wide across all descendant processes in real-time.
- **A-Tier Scalability**: Transitioned from global kernel locks to **Fine-Grained Locking**. IPC, VFS, and Scheduler now support parallel execution across multiple CPUs, eliminating the 'Big Kernel Lock' bottleneck.
- **Core Abstractions**: Shell (44), Editor (23), Scheduler (23), and ZiqaFs (23) form the central nervous system — the graph reveals these as the most structurally coupled components.
- **Surprising Connections**: Clear semantic bridges between syscall interrupt vectors and handlers, eBPF engine ↔ verifier, and architecture diagrams ↔ implementation code.
- **Token Efficiency**: The knowledge graph achieves **75.4x token reduction** per query compared to naive full-corpus context — a 238x reduction for authentication queries.
- **Documentation Coverage**: [`docs/ARCHITECTURE_TARGET.md`](docs/ARCHITECTURE_TARGET.md) documents the build configuration (target specs, linker, bootimage, toolchain, build scripts). The 34 newly documented nodes are now connected to the knowledge graph.
- **Community Boundaries**: 122 architectural communities identified with a published refactoring map in [`docs/architecture/community-boundaries.md`](docs/architecture/community-boundaries.md).

It is **not** a production-ready OS, but an architectural laboratory for exploring the limits of safety-critical systems.

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 🩹 What We've Fixed

| Issue | Fix |
| :--- | :--- |
| **VGA color mapping** | Corrected syntax error in VGA color palette initialization for proper CP437-safe rendering. |
| **Linear framebuffer (LFB)** | Enabled and initialized LFB for high-res graphics; fixed hardware blitting path. |
| **nwm-test compilation** | Resolved linker errors and compilation failures; ensured kernel threads have proper stacks. |
| **nwm-test compositor hang** | Fixed deadlock by spawning compositor and client as separate tasks instead of blocking on the same thread. |
| **Missing SHM/IPC syscalls** | Implemented missing native SHM and IPC syscalls; synchronized Zig client ABI with kernel. |
| **Compositor heap exhaustion** | Resolved heap exhaustion panic by pre-registering the demo surface. |
| **Kernel heap size** | Increased kernel heap to 32MiB to support compositor backbuffers and prevent OOM. |
| **Recursive syscall dispatch** | Removed recursive `dispatch_syscall` from `LinuxAbiPlugin` to prevent stack overflow. |
| **SMP/APIC/Memory compilation errors** | Fixed kernel compilation errors across SMP, APIC, and Memory subsystems during eBPF integration. |
| **Scheduler hardening** | Added `without_interrupts` blocks and interrupt-safe scheduling to prevent race conditions. |
| **Boot sequence logging** | Added extensive serial logging to `init.rs` and `main.rs` for debugging boot failures. |
| **VirtIO PCI register offsets** | Corrected VirtIO network device PCI register offsets for proper device detection. |
| **Exit handling in syscall dispatch** | After `sys_exit`, drops process lock and calls `SCHEDULER.schedule()` to prevent return trampoline resuming a dead process. |
| **Double fault on context switch** | Added kernel stack allocation in `spawn_elf()` for all processes (including WASM); static frame allocator array replaced early heap allocation. |
| **VFS not initialized panic** | Moved VFS initialization before self-tests in `init_subsystems()` to prevent uninitialized access during snapshot tests. |
| **WASM loop control flow** | Replaced malformed WASM binary with valid loop module; inline interpreter tests avoid scheduler deadlock. |
| **Capability I/O test failure** | Fixed test to use `pipe:` scheme (which exists and supports read/write) instead of non-existent file. |
---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## ✨ What We've Added

| Feature | Description |
| :--- | :--- |
| **SMP (Multi-Processor)** | AP boot via INIT-SIPI-SIPI protocol, per-CPU data via GS.base MSR, IPI reschedule/TLB shootdown, APIC timer. |
| **ACPI Table Parsing** | RSDP/MADT/FADT parsing for processor topology and interrupt routing (KernelAcpiHandler). |
| **PCI Enumeration** | Full CF8/CFC bus scan (0–255 × 0–31 × 0–7), BAR decoding, class-based device discovery. |
| **Device Driver Model** | Generic `Driver` trait with PCI match/init lifecycle, global `DeviceManager`, block device registry. |
| **PS/2 Mouse Driver** | 3-byte packet decode, signed delta clamping, integrated with NWCC desktop for cursor/window interaction. |
| **FAT32 (Read-Only)** | Pure Rust FAT32: MBR/BPB parsing, cluster chain walking, short-name directory parsing, recursive VFS mount. |
| **Socket Stack (TCP/UDP)** | Socket state machine via smoltcp, connect/send/recv/close, AF_UNIX loopback, UDP datagrams. |
| **eBPF Hash Maps** | Linear-probing hash map type with insert/update/delete, shared via `Arc<BpfMap>`. |
| **eBPF Tail Calls** | `bpf_tail_call` with ProgArray map type for chaining eBPF programs. |
| **eBPF Tracepoints** | Attach/detach/run for SyscallEntry/SyscallExit tracepoints with verifier gating. |
| **eBPF Bounded Loops** | Instruction-limit-based loop detection instead of strict backward-jump ban. |
| **eBPF Helpers** | `bpf_get_current_comm`, `bpf_probe_read` for safe kernel memory access. |
| **COW Fork** | Full copy-on-write fork with per-process page table cloning, `handle_cow_fault()`. |
| **VMA System** | `Vma` struct with `find_free_range()`, replacing static region model for mmap/brk. |
| **NWCC Desktop Demo** | 80×25 VGA text-mode window manager with 6 apps, mouse/keyboard interaction, double-buffered rendering, animated starfield, taskbar, start menu. |
| **Userspace DRM Driver** | Skeleton userspace DRM driver with IPC ioctl dispatch and MMIO mapping via `syscall_dev_map`. |
| **Userspace Net Driver** | Skeleton userspace VirtIO-Net driver with port I/O and IRQ wait via `syscall_dev_irq_wait`. |
| **sys_stat / sys_pipe** | Finalized POSIX syscall additions for filesystem stat and inter-process pipe communication. |
| **sys_waitpid status** | Corrected child exit status reporting in `sys_waitpid`. |
| **POSIX ABI Cleanup** | Synchronized waitpid options, fixed compiler issues, consolidated POSIX implementation. |
| **Memory Compression** | LZ4-based page compression with Shannon-entropy classifier. Pages analyzed for compressibility (zero-page, short-pattern, text/code, random detection) and transparently decompressed on page fault via COMPRESSED_BIT PTE flag. |
| **Compression Daemon** | Background daemon (5s cycle, 64-page budget) scanning all live processes, classifying pages, and compressing cold pages into a `CompressedPageStore`. On-demand via `compress [N]` shell command. |
| **Snapshot Persistence** | Full process state serialization (CPU registers, VMAs + page contents, FD table, metadata) to FAT32 in ZSNP binary format. LZ4-compressed before write. `snap`, `ls-snap`, `rm-snap` shell commands. |
| **Instant-On Resume** | Boot-time restoration of all saved snapshots — `restore_all_at_boot()` spawns placeholder processes and overwrites their state from `/fat/snapshots/*.snap` files, resuming execution where left off. |
| **eBPF Hooks for Compression** | `ShouldCompress`, `AfterDecompression`, `PageAccess` hook points for eBPF-driven compression policy. |
| **Debug Instrumentation** | Serial logging across boot sequence, missing exception handlers, `without_interrupts` scheduler hardening. |

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 🧠 Knowledge Graph Insights
Run `/graphify` to generate an interactive architectural knowledge graph. The latest analysis (1194 nodes · 1621 edges · 122 communities) reveals:

### 🌟 God Nodes (Most Connected Components)
<div align="center">
  <img src="assets/scheduler.svg" alt="Scheduler Diagram" width="200"/>
</div>
1. [`Shell`](src/shell.rs) - 44 edges (interactive interface layer, command dispatch hub)
2. [`Editor`](src/edit.rs) - 23 edges (console text editor)
3. [`Scheduler`](src/process/scheduler.rs) - 23 edges (MLFQ task scheduler)
4. `read_block()` - 23 edges (ZiqaFS block I/O)
5. [`ZiqaFs`](src/fs/ziqafs.rs) - 23 edges (filesystem implementation)
6. `read_inode()` - 21 edges (inode reading)
7. `ZiqaKernel` - 18 edges (architecture overview concept)
8. [`Vfs`](src/fs/vfs.rs) - 16 edges (virtual filesystem switch)
9. `write_block()` - 16 edges (ZiqaFS block I/O)
10. `Master Architecture Dashboard` - 13 edges (top-level architecture concept)

### 🔗 Surprising Connections (Non-obvious Dependencies)
<div align="center">
  <img src="assets/syscall.svg" alt="Syscall Diagram" width="250"/>
</div>
- `int 0x80 Syscall Gate` → `int 0x80 Syscall Gate Handler` [INFERRED] — architectural SVG diagram directly connected to the actual interrupt handler code
- `eBPF Engine` → `eBPF Verifier Engine` [INFERRED] — design docs linked to implementation across different source files
- `Kernel Architecture Spectrum` → `MLFQ Scheduler Subsystem` [INFERRED] — diagram concept mapped to concrete scheduler code

### 🏘️ Community Structure
The system organizes into 122 architectural communities. Key communities include:
- **Linux Syscall Handlers** (Community 0): 65 syscall stubs — first split applied, now delegates to family modules
- **ZiqaFS Implementation** (Community 1): journaling filesystem with block/inode/dir/journal layers
- **Scheduler Core** (Community 2): MLFQ scheduler with priority boosting and signal delivery
- **Shell and Utilities** (Community 3): shell core with built-in commands (cat, ls, cd, ps, etc.)
- **Process Management** (Community 4): FdTable, Process, AbiKind, CpuState
- **Page Cache System** (Community 10): cache pages with LRU tracking
- **eBPF VM** (Community 22): bytecode verifier and interpreter

### ⚠️ Identified Improvement Areas
<div align="center">
  <img src="assets/capability-flow.svg" alt="Capability Flow" width="200"/>
</div>
- **Low Cohesion Subsystems**: Community 0 (Linux Syscall Handlers) shows cohesion 0.028 — indicating strong need for further modularization
- **Weakly-Connected Nodes**: 229 nodes with ≤1 connection — many are build scripts and auxiliary config files now partially addressed by ARCHITECTURE_TARGET.md
- **Cross-Community Bridges**: `Pid` connects `Process Management` ↔ `Scheduler Core`; `kernel_main()` bridges 7 communities
- **Bridge Points**: `Kernel Core` connects 6 different communities (POSIX Layer, Shared Memory IPC, Timer System, etc.)

View the full interactive graph: [graphify-out/graph.html](graphify-out/graph.html)
Read the detailed analysis: [graphify-out/GRAPH_REPORT.md](graphify-out/GRAPH_REPORT.md)

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 🏗️ Architecture Spectrum
ZiqaKernel prioritizes modularity and safety research over industrial-scale stability.

<div align="center">
  <img src="assets/os-spectrum.svg" alt="Kernel Architecture Spectrum" width="100%">
</div>

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 📊 Comparative Analysis: Capability Matrix

<div align="center">
  <img src="assets/capability-matrix.svg" alt="Capability Matrix" width="100%">
</div>

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 🧠 Design Philosophy: Why We Build This
ZiqaKernel exists because modern OS architecture is stagnant. We solve systemic issues via:

1.  **Memory Safety**: Transitioning from C-based "best effort" safety to Rust's **compile-time guarantees**.
2.  **Plugin-based ABI**: Isolating process interactions into plugin modules, enabling experimentation with Linux ELF, WASM, and Native runtimes.
3.  **Hybrid Rust/Zig**: Using Zig's `ReleaseFast` for graphics hot paths (blitter) while retaining Rust's safety guarantees for the rest of the kernel.
4.  **Experimental Security**: Implementing a **Capability Space** approach, reducing the blast radius of compromised processes.

<div align="center">
  <img src="assets/vfs_capability.svg" alt="VFS Capability" width="250"/>
  <img src="assets/ebpf-logic.svg" alt="eBPF Logic" width="250"/>
</div>

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 📈 Knowledge Graph Benchmark

The graphify graph provides dramatic token savings for codebase queries:

<pre>
graphify token reduction benchmark
──────────────────────────────────────────────────
  Corpus:          95,924 words → ~127,898 tokens (naive)
  Graph:           1,194 nodes, 1,621 edges
  Avg query cost:  ~1,697 tokens
  Reduction:       75.4x fewer tokens per query

  Per question:
    [238.6x] how does authentication work
    [74.1x] what is the main entry point
    [58.8x] how are errors handled
    [58.8x] what are the core abstractions
</pre>

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 🛠️ Engineering Audit Findings (May 2026)

Following a comprehensive forensic audit, the project status has been updated to reflect its current experimental nature.

| Component | Maturity | Engineering Assessment |
| :--- | :--- | :--- |
| **SMP** | **Complete** | **Multi-Processor Boot & IPI Infrastructure.** AP boot via INIT-SIPI-SIPI (16-bit→64-bit real-mode trampoline at 0x7000). Per-CPU structs via GS.base MSR with O(1) lock-free access. Up to 64 CPUs. Dedicated IPI vectors for rescheduling (0x34) and TLB shootdown (0x35) with busy-wait ACK. APIC timer calibration against the PIT. |
| **APIC** | **Complete** | **Local APIC + I/O APIC.** Memory-mapped register access, interrupt redirection (IRQ→vector→APIC ID), masking/unmasking, four IPI modes (INIT/SIPI/Fixed/Broadcast), timer setup. Dedicated `enable_lapic_in_bsp()` via IA32_APIC_BASE MSR. |
| **ACPI** | **Complete** | **Table Parsing Stack.** RSDP search (EBDA/BIOS), RSDT/XSDT walk, MADT parsing (Local APIC address, I/O APIC list with GSI bases, interrupt source overrides, legacy PIC presence), processor topology extraction. Global `ACPI_INFO` singleton. |
| **PCI** | **Complete** | **Legacy PCI Bus Enumeration.** CF8/CFC config space access, bus 0–255 × device 0–31 × function 0–7 scan, vendor/device ID, class/subclass/prog-if, BAR decoding (6 BARs), interrupt line/pin. `find_device()`, `find_by_class()`, bus mastering helpers. |
| **Device Model** | **Complete** | **Generic Driver Framework.** `Driver` trait (`name()`, `pci_match()`, `init()`) with PCI auto-probe. Global `DeviceManager` iterates drivers in order, first match wins. Block devices registered by name in `BlockRegistry`. Drivers: VirtIO Net, VirtIO Block (new + legacy). |
| **Microkernel** | **Hardened** | **Ring 3 Userspace Drivers.** Graphics (DRM) and Network drivers transitioned to Ring 3. Hardware Capability system (`DeviceIo`) enforces secure access to MMIO and I/O ports. Userspace driver skeletons for DRM (IPC-based ioctl dispatch) and VirtIO-Net (port I/O + IRQ wait). |
| **Boot & HAL** | **Functional** | Reliable BIOS/UEFI boot. **Three-stage VGA boot pipeline with CP437-safe animation.** |
| **Capability** | **A+ Tier** | **Instant Recursive Revocation.** Implemented a system-wide Revocation Tree tracking parent-child capability delegation. `sys_cap_revoke` instantly severs access for all descendants across all processes. |
| **Scheduler** | **A-Tier** | **Scalable Decoupled Architecture.** Transitioned from global Mutex to fine-grained per-process locking + RwLock process table. Supports multi-core scheduling without global lock contention. Interrupt-safe scheduling with `without_interrupts` blocks. |
| **Scalability** | **Hardened** | **Eliminated Big Kernel Lock.** Fine-grained locking implemented across IPC (per-channel), VFS (per-file lookup), and Scheduler. Ready for massive multi-core scaling. |
| **Privilege** | **Hardened** | Full Ring 3 user/kernel isolation complete. TSS/context-switch hardening, ELF memory mapping audit, and `rflags` sanitization (IOPL/TF/DF/NT/RF/VM/AC cleared, IF enforced) across all kernel→user paths (`iretq`, `sysretq`, Rust handler). Paranoid register zeroing on every transition. **SMEP (CR4.20), SMAP (CR4.21), UMIP (CR4.11)** enabled after CPUID detection with CR4 write-back verification. `copy_from_user`/`copy_to_user` with page-table validation + STAC/CLAC brackets. |
| **Syscall ABI** | **Complete** | 115+ syscalls (incl. native ZIQA_CAP/SIG handlers); full libposix ABI foundation (finalized `sys_waitpid` status reporting, `sys_stat`, `sys_pipe`). Userspace process launch (`spawn_elf`, `exec_process`), `sys_brk` with page allocation, `sys_execve` with VFS binary loading, and int 0x80 rewritten in assembly (replaced `extern "x86-interrupt"` with `int80_entry` global_asm trampoline). |
| **Memory** | **Enhanced** | **Per-Process Page Tables + COW Fork + VMA.** Full per-process address spaces with shared kernel entries (indices 256–511). COW fork: `make_user_leaf_readonly()`, `clone_user_table_tree()`, `cow_fork_parent()`, `handle_cow_fault()`. VMA collection replacing static regions. `sys_mmap` uses VMA manager. `sys_brk` with page-aligned heap expansion. |
| **VirtIO** | **Experimental** | `virtio-drivers` crate MMIO transport for block devices alongside custom VirtIO net/block drivers. Comparative driver development path. |
| **Hybrid FFI** | **Functional** | Rust → Zig C-ABI blitter for framebuffer ops; linked via build.rs + build.zig. |
| **eBPF VM** | **Obsidian-Tier** | Advanced VM with **Array/Hash/RingBuf/ProgArray Maps**, **512B VM Stack**, **SMP-aware helpers** (`bpf_get_smp_processor_id`), **64-bit immediate loads**, **bounded loops**, **tail calls** (`bpf_tail_call` w/ ProgArray), **tracepoint attach/detach** (SyscallEntry/SyscallExit), **kCFI verification**, and **`bpf_probe_read`** for safe kernel memory access. |
| **FAT32** | **Experimental** | **Read-only FAT32 filesystem driver** (pure Rust, no external crate). MBR partition scanning, BPB parsing, cluster chain traversal, short-name directory entries. Recursive VFS mounting under `/fat`. |
| **PS/2 Mouse** | **Complete** | **PS/2 Mouse Driver.** 3-byte packet decode with button state (left/right), signed delta X/Y clamping to 1920×1080. Integrated with the NWCC desktop for cursor control. |
| **Socket Stack** | **Experimental** | **TCP/UDP socket layer** via smoltcp. Socket state machine (Created→Bound→Listening→Connected→Closed). `AF_UNIX` loopback pairs, `AF_INET`/`SOCK_STREAM` TCP with connect/send/recv/close, `SOCK_DGRAM` UDP. Lazy socket creation. |
| **Shell** | **Modernized** | Zero-alloc prompt, real parser (quotes/escapes/env expansion), 40+ builtins via command registry, job control (`bg`/`fg`/`jobs`), tab completion, arrow history, ANSI colors. **New: `bench`, `test`, `compress`, `snap`, `ls-snap`, `rm-snap` commands.** |
| **Graphics** | **Demos** | DOOM fire + Tetris on bare metal; **NWCC Desktop Demo** — 80×25 VGA text-mode floating window manager with 6 application views (Terminal, System Monitor, File Manager, Network Dashboard, Text Editor, About), full mouse/keyboard interaction, double-buffered rendering with dirty-rectangle flush, animated starfield desktop, taskbar, start menu, and scanline effect. DRM/KMS driver for future compositor support. |
| **Memory Compression** | **Experimental** | LZ4-based page compression with Shannon-entropy classifier. `CompressedPageStore` with per-page tracking. `COMPRESSED_BIT` PTE flag for transparent on-fault decompression. Background daemon scanning all processes every 5 s (64-page budget). Snapshot persistence to FAT32 with instant-on boot resume. |

> **Audit Conclusion**: ZiqaKernel is a sophisticated architectural scaffold. It is ideally suited for **OS research, compiler-assisted OS design, and hardware-software interface experimentation**.

<div align="center">
  <img src="assets/boot.svg" alt="Boot Process" width="300"/>
</div>

### Three-Stage Boot Pipeline

The boot presentation in [`src/boot_screen.rs`](src/boot_screen.rs) now renders a CP437-safe VGA animation with stage tabs, scanline shimmer, progress telemetry, and a service activation grid:

1. **Stage I — CPU + Interrupts**: firmware handoff, GDT, IDT, PIC, interrupt gate readiness.
2. **Stage II — Memory + Scheduler**: memory map capture, frame allocator, heap, mapper, DRM, scheduler bootstrap.
3. **Stage III — Services + Shell**: ABI registry, VFS/ZiqaFs, consoles, device probes, IPC/SHM, eBPF, shell handoff.

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 🧩 Hybrid Rust/Zig FFI
Performance-critical graphics operations are written in **Zig** (`src/zig/blitter.zig`) and called from Rust via C-ABI FFI:
- **`zig_fill_rect`** — fill rectangles on a 32-bit XRGB8888 framebuffer
- **`zig_blit_bitmap`** — blit bitmaps with source/destination rectangles
- **`zig_scroll_up`** — scroll framebuffer regions upward
- **`zig_clear`** / **`zig_memset32`** / **`zig_memcpy`** — fast memory operations

The Zig module is compiled as a static library (`build.zig`) and linked via `build.rs`. The `src/zig_ffi.rs` module provides safe Rust wrappers. This hybrid approach gives Zig's `ReleaseFast` optimization for graphics hot paths while keeping the rest of the kernel in Rust.

<div align="center">
  <img src="assets/ebpf.svg" alt="eBPF Logic" width="250"/>
</div>

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 🔌 System Interconnectivity
ZiqaKernel connects disparate subsystems through a central **Core ABI Registry**.

<div align="center">
  <img src="assets/system-interconnect.svg" alt="System Interconnect Diagram" width="100%">
</div>

<div align="center">
  <img src="assets/abi-flow.svg" alt="ABI Flow" width="250"/>
</div>

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## ⚙️ Subsystem Deep Dives

### 1. Memory Model
*   **4-Level Paging**: Standard x86_64 paging implementation.
*   **Per-Process Page Tables**: Each user process gets its own L4 page table with kernel entries (256–511) shared by pointer and user entries (0–255) cloned for COW fork. `create_process_page_table()` allocates and initializes per-process L4 frames.
*   **COW Fork**: Full copy-on-write fork support — `make_user_leaf_readonly()` clears writable bits on all user pages, `clone_user_table_tree()` recursively copies user entries, `cow_fork_parent()` orchestrates the sequence (readonly + clone + TLB flush), and `handle_cow_fault()` allocates a new frame, copies data, and remaps as writable.
*   **VMA System**: `Vma` struct (`start`, `end`, `flags`, file-backing info) with `find_free_range()` for mmap allocation. Per-process VMA collections replace the old static region model.
*   **Pre-Mapped ELF Segments**: `map_process_regions()` allocates physical frames and copies ELF binary data into new user pages at load time — no demand paging needed for initial code/data.
*   **Safe Access**: `copy_from_user` routine validates all user-space memory access via page-table walkers.
*   **Demand Paging**: Placeholder mechanism for lazy page allocation and COW resolution.
*   **Frame Allocator**: BootInfo-based physical frame allocator.
*   **Heap Profiler**: Tracks allocation rates, fragmentation, and usage.

<div align="center">
  <img src="assets/memory.svg" alt="Memory Layout" width="250"/>
  <img src="assets/memory-layout.svg" alt="Detailed Memory Layout" width="250"/>
</div>

### 2. Process Lifecycle
*   **State Machine**: `Created → Ready → Running → Blocked → Exited`.
*   **Capabilities**: Each process has a defined `CapabilitySpace` for resource access.
*   **Signals**: SignalState with default dispositions and custom action handlers.
*   **User Process Launch**: `spawn_elf()` loads a static ELF binary into a new per-process page table, resets CPU state and kernel stack, and context-switches to Ring 3.
*   **`exec_process()`**: Full address-space replacement — clears old mappings, creates a fresh page table, loads a new ELF, resets kernel stack and register state.
*   **COW Fork**: Full fork support via per-process page tables. Parent pages marked read-only, child gets cloned user entries, and faults trigger `handle_cow_fault()` to allocate+copy frames.
*   **Exit Handling**: After `sys_exit` dispatch, the handler drops the process lock and calls `SCHEDULER.schedule()`, preventing any return trampoline from resuming a dead process.

<div align="center">
  <img src="assets/ipc.svg" alt="IPC Mechanisms" width="300"/>
</div>

### 3. ABI Plugin System
*   **Linux ABI**: ELF Loader + Syscall table (111 implemented/stubbed across fs, process, memory, time, net, misc).
*   **WASM ABI**: WASI host functions for execution with function table and local declarations.
*   **eBPF Verifier**: Bytecode verification with kCFI and kernel-address sanitizer support.

<div align="center">
  <img src="assets/ebpf.svg" alt="eBPF Logic" width="250"/>
</div>

### 4. Filesystem Hierarchy
*   **VFS Layer**: Virtual filesystem switch abstraction with mount support.
*   **ZiqaFS**: Journaling filesystem with block allocation, inode management, and directory operations.
*   **FAT32**: Support for host-editable FAT32 partitions (read-only no_std bridge).
*   **Page Cache**: Accelerated file access through LRU-based caching with stats tracking.
*   **RamFS**: In-memory filesystem for temporary storage.

<div align="center">
  <img src="assets/fs-hierarchy.svg" alt="Filesystem Hierarchy" width="300"/>
</div>

### 5. Interrupt Handling
*   **IDT Setup**: Interrupt descriptor table configuration with PIC remapping.
*   **Exception Handling**: Page fault, double fault, breakpoint, and general protection fault handlers.
*   **IOAPIC**: Advanced interrupt controller support for IRQ routing.
*   **int 0x80 Assembly Trampoline**: Replaced the `extern "x86-interrupt"` handler with a `global_asm` `int80_entry` that pushes GPRs in CpuState order, calls `rust_syscall_handler`, sanitizes rflags (clears IOPL/TF/DF/NT/RF/VM/AC, enforces IF), clears XMM registers, and returns via `iretq`.

<div align="center">
  <img src="assets/interrupts.svg" alt="Interrupt Layout" width="300"/>
  <img src="assets/interrupt-layout.svg" alt="Detailed Interrupt Layout" width="300"/>
</div>

### 6. Network Stack
*   **Virtio-Net**: Network device driver with RX/TX virtqueues, ARP/ICMP response.
*   **TCP/IP Stack**: Gateway, IP addressing, MAC, polling loop.
*   **DNS**: IPv4 address resolution.
*   **HTTP**: Client with status parsing and body extraction.

### 7. Capability System
*   **Capability Space**: Per-process capability tokens with grant, revoke, and permission checking.
*   **Resource Kinds**: Different resource types (files, devices, IPC) with read/write/full permissions.

### 8. SMP / Multi-Processor
*   **AP Boot**: Real-mode trampoline at 0x7000 transitioning 16-bit → 32-bit → 64-bit long mode, following the INIT-SIPI-SIPI protocol. APs enter `ap_entry()` and initialize their GDT, APIC timer, then idle-schedule.
*   **Per-CPU Data**: `PerCpu` struct (cpu_id, apic_id, current_pid, kernel stack, CR3, ticks) accessed O(1) lock-free via GS.base MSR. Supports up to 64 CPUs tracked in a static pointer array.
*   **IPI Infrastructure**: Dedicated IPI vectors for rescheduling (0x34) and TLB shootdown (0x35). `send_reschedule_ipi_all()`, `tlb_shootdown_all()` with per-CPU flag + busy-wait ACK. `handle_tlb_shootdown()` flushes remote TLB.
*   **APIC Timer**: PIT-calibrated per-CPU APIC timer for local interrupt generation. See [`src/arch/x86_64/apic.rs`](src/arch/x86_64/apic.rs), [`src/arch/x86_64/smp.rs`](src/arch/x86_64/smp.rs), [`src/arch/x86_64/per_cpu.rs`](src/arch/x86_64/per_cpu.rs).

### 9. ACPI & PCI Enumeration
*   **ACPI Tables**: RSDP search (EBDA/BIOS area 0xE0000–0xFFFFF), RSDT/XSDT walk. MADT parsing for Local APIC address, I/O APIC list with GSI bases, interrupt source overrides, legacy PIC presence. Processor topology (BSP + AP list with APIC IDs). [`src/drivers/acpi.rs`](src/drivers/acpi.rs).
*   **PCI Bus Scan**: Legacy CF8/CFC config space access enumerating buses 0–255 × devices 0–31 × functions 0–7. Reads vendor/device ID, class/subclass, BARs (6 per function), interrupt line/pin. `find_device()`, `find_by_class()`, BAR decoding helpers. [`src/drivers/pci.rs`](src/drivers/pci.rs).

### 10. Device Driver Model
*   **Driver Trait**: `Driver { name(), pci_match(), init() }` — drivers register via `DEVICE_MANAGER.register_driver()`. `scan_and_match()` iterates PCI devices and probes each driver in order; first match wins.
*   **Block Registry**: `BLOCK_DEVICES` global registry maps device names (`"vda"`, `"hda"`) to `Arc<dyn BlockDevice>`. `register()`, `get()`, `first()`, `print_devices()`. Decouples filesystem code from specific block drivers.
*   **VirtIO Block (New)**: MMIO-based VirtIO block driver via the `virtio-drivers` crate. Matches vendor 0x1AF4 / device 0x1001, maps MMIO BAR, initializes `VirtIOBlk`, registers as `"vda"`.
*   **Userspace Drivers**: Skeleton DRM driver (IPC-based ioctl dispatch with MMIO mapping via `syscall_dev_map`) and VirtIO-Net driver (port I/O + IRQ wait via `syscall_dev_irq_wait`), demonstrating the microkernel driver isolation model. See [`src/userspace/drm_driver.rs`](src/userspace/drm_driver.rs), [`src/userspace/net_driver.rs`](src/userspace/net_driver.rs).

### 11. IPC Mechanisms
*   **Channels**: Bounded buffer channels with send/recv operations.
*   **Signals**: Signal queue with push/pop.
*   **Shared Memory**: SHM regions with real physical frame mapping and cross-process attachment.
*   **io_uring**: High-performance asynchronous ring buffer for I/O and IPC.

### 12. Unified Zero-Copy Pipeline (Fast Path)
Implemented a "Single-Copy" architecture that unifies **io_uring**, **SHM**, and **VirtIO** drivers:
- **Direct DMA**: Network packets and disk blocks are DMA'd directly into Shared Memory (SHM) regions.
- **Bypassing CPU**: Data flows from hardware to the compositor/database without the CPU ever touching the payload.
- **Fast-Path Syscalls**: Native `shm_get`, `shm_at`, and `io_uring_submit` syscalls provide a low-latency path for high-throughput userspace apps.
- **Performance**: Achieves near-hardware speeds for network-to-graphics data flow.

### 13. FAT32 Filesystem
A pure Rust (no external crate) read-only FAT32 driver ([`src/fs/fat32.rs`](src/fs/fat32.rs)):
- **MBR Parsing**: Scans 4 primary partition entries for FAT32 types (0x0B/0x0C CHS/LBA).
- **BPB Parsing**: Bytes per sector, sectors per cluster, FAT regions, root cluster.
- **Cluster Chain Walking**: 4-byte FAT entries (masked 28-bit), chain traversal with EOC detection (≥ 0x0FFFFFF8) and infinite-loop guard.
- **Directory Parsing**: Short 8.3 names (skips LFNs, deleted entries 0xE5, end sentinel 0x00), extracts name, directory flag, first cluster, file size.
- **Recursive Mounting**: `walk_and_mount()` recursively walks the directory tree and mounts files into the VFS at a configurable prefix (e.g., `/fat`).

### 14. PS/2 Mouse Driver
Full PS/2 mouse initialization and interrupt handler ([`src/drivers/ps2_mouse.rs`](src/drivers/ps2_mouse.rs)):
- **Initialization**: Enable auxiliary device (port 0x64 command 0xA8), enable interrupts, set defaults (0xF6), enable reporting (0xF4).
- **3-Byte Packet Decode**: Button flags (left/right), signed delta X/Y clamped to 1920×1080.
- **Public API**: `get_mouse_pos()` → (i32, i32), `get_mouse_btn()` → u8 bitmask.
- Integrated with the NWCC desktop for cursor control and window interaction.

### 15. Socket Stack (TCP/UDP)
Network socket layer ([`src/net/socket.rs`](src/net/socket.rs)) built on smoltcp:
- **Socket State Machine**: Created → Bound → Listening → Connected → Closed. Supports `AF_UNIX` (loopback pairs) and `AF_INET`/`SOCK_STREAM` (TCP) + `SOCK_DGRAM` (UDP).
- **TCP Operations**: `tcp_connect()` (3-way handshake with 5s timeout), `tcp_send()` (`send_slice`), `tcp_recv()` (`recv_slice` with polling), graceful `close()` (FIN).
- **UDP Operations**: `udp_send()` / `udp_recv()` with sender metadata, lazy socket creation.
- **Listener API**: `find_listener()` / `find_any_listener()` for incoming connection acceptance.
- Global singleton `SOCKETS: Mutex<SocketManager>`.

### 16. DOOM Fire Effect
A classic fire propagation algorithm ([`src/doom.rs`](src/doom.rs)) using the Zig blitter for the hot-path fire step and rendering:
- 37-color palette: black → red → orange → yellow → white
- Renders to either a real framebuffer or ASCII serial output
- Driven by the Zig `zig_fill_rect` / `zig_blit_bitmap` routines

### 17. Tetris Game
Full graphical Tetris ([`src/tetris.rs`](src/tetris.rs)) running on bare metal:
- 7 standard Tetris pieces (I, O, T, S, Z, J, L)
- Calls the optimized Zig blitter FFI for clear/draw/fill_rect
- Downsampled from a virtual 32-bit framebuffer to the VGA text screen at `0xb8000`
- Score tracking, piece locking, line clearing

### 18. Text Editor
A nano-like console text editor ([`src/edit.rs`](src/edit.rs)) with:
- Open, edit, and save files on ZiqaFS
- Cursor navigation (arrows, home, end)
- Insert/overwrite mode toggle
- Modified-file indicator, scroll support
- 80×24 character grid

### 19. DRM/KMS Driver
A minimal Direct Rendering Manager / Kernel Mode Setting driver ([`src/drivers/drm.rs`](src/drivers/drm.rs)) providing the core ioctls needed for graphics compositors:
- `DRM_IOCTL_MODE_FB_CREATE` / `DESTROY` — framebuffer object management
- `DRM_IOCTL_MODE_PAGE_FLIP` — vsync page flipping
- `DRM_IOCTL_MODE_GETRESOURCES` — enumerate CRTCs, connectors, encoders
- Pixel formats: XRGB8888, ARGB8888, RGB565

### 20. eBPF "Obsidian-Tier" VM
Extended Berkeley Packet Filter subsystem ([`src/ebpf/`](src/ebpf/)) for running safe, verified kernel-space programs:
- **Verifier** — kCFI (Control-Flow Integrity), bounded loop detection (instruction limit, not strict backward-jump ban), and **stack-relative bounds checking** (safe R10 access).
- **VM Infrastructure** — interpreter for verified eBPF bytecode with:
    - **512B Local Stack** for local variables and structure passing.
    - **4 Map Types**: `Array` (O(1) index), `Hash` (linear-probing with used-byte flag), `RingBuf` (raw byte buffer), `ProgArray` (for tail calls) — all shared via `Arc<BpfMap>`.
    - **Tail Calls**: `bpf_tail_call` with `ProgArray` map type, allowing eBPF program chaining.
    - **64-bit Immediates** (`LD_IMM_64`) for pointer manipulation.
    - **Advanced Helpers**: `bpf_map_lookup/update`, `bpf_get_current_pid_tgid`, `bpf_get_smp_processor_id`, `bpf_get_current_comm`, and `bpf_probe_read` (safe kernel memory reads).
- **Tracepoint Attachment**: `EbpfAttachments` singleton with `attach()` (verifies before adding), `detach()` (removes by index), and `run()` (executes all programs matching a `TracepointType`: `SyscallEntry`/`SyscallExit`).
- **Use cases**: SMP-aware tracing, dynamic kernel instrumentation, networking filters, and real-time security auditing.

### 21. Performance Suite
Built-in benchmarking utilities ([`src/perf.rs`](src/perf.rs)) using x86_64 RDTSC:
- Cycle-accurate measurement of scheduler, memory, and I/O operations
- Page cache hit/miss benchmarks
- Heap profiling (allocation count, current/peak usage, fragmentation)
- Heap profiler tracks allocation sites

### 22. Shell Features
The interactive shell ([`src/shell.rs`](src/shell.rs) — ~2120 lines) provides a full-featured command environment:
- **Real parser** — character-level tokenizer with single/double quote handling, backslash escapes, `|`/`>`/`<`/`&` operators, and `Err("unclosed quote")` on parse errors
- **Environment variable expansion** — `$?`, `$$`, `$VAR`, `${VAR}` during parse phase
- **Zero-alloc prompt** — stack `[u8; 128]` buffer with manual integer serialization, no heap allocation per prompt
- **Command registry** — `find_builtin` table dispatches 40+ builtins returning `i32`; unrecognized commands fall through to `spawn_elf` binary search
- **Job control** — `bg`, `fg`, `jobs` with `SCHEDULER.send_signal(pid, SIGCONT)`, background process tracking, and dead-job cleanup via `poll_jobs()`
- **Tab autocomplete** — cycle through matching commands
- **Arrow key history** — browse last 50 commands via `Vec<String>` (no fixed-size byte arrays)
- **Redirection** — `>` (truncate), `>>` (append), `<` (read) via VFS
- **ANSI color output** — syntax-highlighted prompts and errors
- **All commands**: `help`, `uptime`, `ps`, `spawn`, `spawnelf`, `exec`, `kill`, `sleep`, `meminfo`, `diskinfo`, `netstat`, `klog`, `doom`, `tetris`, `reboot`, `echo`, `clear`, `edit`, `ls`, `cd`, `pwd`, `mkdir`, `dir`, `rm`, `rmdir`, `cat`, `ping`, `wget`, `ifconfig`, `mv`, `cp`, `touch`, `stat`, `du`, `alias`, `export`, `history`, `bg`, `fg`, `jobs`, `bench`, `test`, `compress`, `snap`, `ls-snap`, `rm-snap`

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 💡 Documentation Coverage

As part of the ongoing effort to reduce weakly-connected nodes, the following documentation has been created:

| Document | Description | Nodes Connected |
| :--- | :--- | :--- |
| [`docs/ARCHITECTURE_TARGET.md`](docs/ARCHITECTURE_TARGET.md) | Build configuration (target spec, linker, bootimage, toolchain, build scripts, Cargo, Docker) | 34 |
| [`docs/architecture/community-boundaries.md`](docs/architecture/community-boundaries.md) | Refactoring map for low-cohesion communities | N/A |
| [`BUILD_OPTIMIZATIONS.md`](BUILD_OPTIMIZATIONS.md) | Build speed tuning (codegen units, incremental, sccache) | N/A |

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 🎨 Art Assets
Architectural diagrams and flowcharts in `assets/` provide visual overviews of kernel subsystems:

| Asset | Description |
| :--- | :--- |
| `logo.svg` | ZiqaKernel project logo |
| `arch.svg` | System architecture overview |
| `boot.svg` | Three-stage boot pipeline |
| `memory*.svg` | Memory layout diagrams |
| `interrupt*.svg` | Interrupt handler layout |
| `scheduler*.svg` | MLFQ scheduler design |
| `syscall.svg` | Syscall gate flow |
| `vfs_capability.svg` | VFS + capability interaction |
| `fs-hierarchy.svg` | Filesystem hierarchy |
| `ipc.svg` | IPC mechanism overview |
| `ebpf*.svg` | eBPF engine + verifier logic |
| `capability-flow.svg` | Capability grant/revoke flow |
| `capability-matrix.svg` | Comparative capability matrix |
| `system-interconnect.svg` | Full system interconnect diagram |
| `master-dashboard.svg` | Master architecture dashboard |
| `os-spectrum.svg` | OS architecture spectrum |
| `abi-flow.svg` | ABI plugin dispatch flow |
| `pagefault.svg` | Page fault handling flow |

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 🚀 Quick Start
### Build & Run
```bash
make build     # Debug build (~1-2s incremental)
make run       # Build + boot image + QEMU (serial stdio)
make run-gui   # Run with graphical display (for DOOM/Tetris)
make release   # Release build with -O3
make test      # Run test suite
make clean     # Remove build artifacts
```

### Dev Environment
```bash
docker compose run dev
```

### Zig FFI (required for DOOM/Tetris)
```bash
make zig-check   # Verify Zig blitter compiles independently
# Zig module is linked automatically during `cargo build`
```

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 📈 Technical Roadmap

### Completed Milestones (May–June 2026)

- **SMP + APIC** — Multi-processor boot (INIT-SIPI-SIPI), per-CPU data via GS.base, IPI infrastructure (reschedule + TLB shootdown), APIC timer.
- **ACPI + PCI** — Full RSDP/MADT/FADT table parsing, PCI CF8/CFC bus enumeration with BAR decoding.
- **Device Driver Model** — Generic `Driver` trait with PCI match/init lifecycle, block device registry, VirtIO block (MMIO).
- **PS/2 Mouse Driver** — 3-byte packet decode, signed delta, 1920×1080 clamping, integrated with NWCC.
- **FAT32 (Read-Only)** — Pure Rust FAT32: MBR/BPB parsing, cluster chains, short-name directories, recursive VFS mounting.
- **Socket Stack** — TCP/UDP via smoltcp, socket state machine, AF_UNIX loopback, connect/send/recv/close.
- **eBPF Maps** — Array, Hash (linear-probing), RingBuf, ProgArray map types with `Arc<BpfMap>` sharing.
- **eBPF Tail Calls** — `bpf_tail_call` with ProgArray dispatch, bounded loops via instruction limit.
- **eBPF Tracepoints** — Attach/detach/run lifecycle for `SyscallEntry`/`SyscallExit` tracepoints.
- **NWCC Desktop Demo** — 80×25 VGA text-mode window manager with 6 apps, mouse/keyboard interaction, double-buffered rendering, starfield desktop, taskbar, start menu.
- **COW Fork** — Per-process page table cloning, `make_user_leaf_readonly()`, `clone_user_table_tree()`, `handle_cow_fault()`.
- **VMA System** — `Vma` struct with `find_free_range()` for mmap allocation, replacing static region model.
- **Userspace Drivers (Skeleton)** — DRM driver (IPC ioctl dispatch + MMIO mapping) and VirtIO-Net driver (port I/O + IRQ wait).
- **Debug Instrumentation** — Extensive serial logging in boot sequence, missing exception handlers, interrupt-safe scheduler hardening with `without_interrupts` blocks.
- **POSIX Finalization** — `sys_waitpid` status reporting, `sys_stat`, `sys_pipe`, POSIX ABI cleanup.
- **SMEP/SMAP/UMIP** — CR4 bits enabled after CPUID, `copy_from_user`/`copy_to_user` with STAC/CLAC, CR4 write-back verification.
- **Privilege Separation** — Full Ring 3 user/kernel isolation, `rflags` sanitization, register zeroing on all kernel→user transitions.
- **A-Tier Scalability** — Big Kernel Lock eliminated; fine-grained locking in IPC (per-channel), VFS (per-file), Scheduler (per-process).
- **Microkernel Phase 1** — `DeviceIo` capability, hardware access syscalls (port I/O, MMIO map, IRQ wait), kernel DRM gateway.
- **Capability Space Enforcement** — `ResourceKind` checks on all syscall paths (Files, IPC, Network, DeviceIO).
- **Instant Recursive Revocation** — Revocation Tree with `sys_cap_revoke` system-wide capability severance.
- **Unified Zero-Copy Pipeline** — `io_uring` + `SHM` + `VirtIO` DMA bypassing CPU for network-to-graphics flow.
- **Syscall ABI** — 115+ syscalls, int 0x80 assembly rewrite, `exec_process`, `sys_brk`, `sys_execve`.
- **int 0x80 Assembly Rewrite** — Replaced `extern "x86-interrupt"` with `int80_entry` global_asm trampoline.
- **eBPF "Obsidian-Tier" VM** — kCFI verifier, 512B stack, maps, helpers, bounded loops, tail calls.
- **NWCC Compositor** — SHM-backed buffer sharing, VGA-downsampled architecture, window dragging.
- **Memory Compression Infrastructure** — LZ4 compression via `lz4_flex`, page content classifier (entropy/pattern detection), `CompressedPageStore` with per-page location tracking, `COMPRESSED_BIT` PTE flag for transparent decompression on page fault.
- **Compression Daemon** — Background process scanner (`daemon.rs`) with 5s cycle timer, 64-page budget per cycle, automatic classification + compression of cold pages. Invocable via `compress [N]` shell command.
- **Snapshot Persistence** — Process state serialization (ZSNP v1 binary format: CpuState, VMAs, page contents, FD table, metadata) with LZ4 compression to `/fat/snapshots/{pid}.snap`. Shell commands: `snap <pid>`, `ls-snap`, `rm-snap`.
- **Instant-On Boot Resume** — `restore_all_at_boot()` scans `/fat/snapshots/` at startup, restores all saved process states into spawned placeholders, enabling system state recovery across reboots.

### Future Work (P2)

1. **Memory Compression Production** — Wire the compression daemon into the eBPF hooks system for dynamic policy, add multi-tier compression (LZ4 + Zstd), implement free-list frame deallocation to reclaim compressed-store frames on release.
2. **Snapshot Scheduling** — Add automatic periodic snapshots via a kernel timer, snapshot versioning (keep last N), and incremental snapshots for minimal write overhead.
3. **Userspace Drivers (Production)** — Complete the DRM and VirtIO-Net userspace drivers with real virtqueue management, packet processing, and GPU operation support.
4. **Multi-architecture Support** — Explore aarch64 or RISC-V as additional targets beyond x86_64.
5. **Network Stack Maturity** — Fully wire up TCP listener/accept path, integrate with the shell and filesystem for a complete networking experience.
6. **FAT32 Write Support** — Extend the FAT32 driver with write capabilities (file creation, deletion, modification).
7. **eBPF Production Hardening** — Add concurrent map access safety, RINGBUF consumer API, wider tracepoint coverage across all syscalls.
8. **Performance Optimization** — Profile and optimize the COW fork path, page table cloning, and TLB shootdown latency for real workloads.


<div align="center">
  <img src="assets/master-dashboard.svg" alt="System Dashboard" width="400"/>
</div>

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 🤝 Contributing
We welcome contributions from the community! Whether you're fixing bugs, adding features, or improving documentation, your help is appreciated.

### How to Contribute
1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Development Setup
```bash
# Clone the repository
git clone https://github.com/yourusername/ziqakernel.git
cd ziqakernel

# Set up development environment
docker compose run dev

# Build and test
make build
make run

# Verify Zig blitter compiles (needed for DOOM/Tetris)
make zig-check

# Build with graphical display (for demos)
make run-gui
```

### Prerequisites
- **Rust nightly** (via `rustup` with `rust-toolchain.toml`)
- **QEMU** (for `make run`)
- **Zig** (>= 0.11, for the blitter FFI module — `make zig-check` to verify)
- **NASM** (for assembly stubs)
- Optional: `sccache` for faster rebuilds

### Reporting Issues
Please use the GitHub issue tracker to report bugs or request features.

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 📜 Code of Conduct
Please note that this project is released with a Contributor Code of Conduct. By participating in this project you agree to abide by its terms.

### Our Pledge
We as members, contributors, and leaders pledge to make participation in our community a harassment-free experience for everyone, regardless of age, body size, visible or invisible disability, ethnicity, sex characteristics, gender identity and expression, level of experience, education, socio-economic status, nationality, personal appearance, race, religion, or sexual identity and orientation.

### Our Standards
Examples of behavior that contributes to creating a positive environment include:
- Demonstrating empathy and kindness toward other people
- Being respectful of differing opinions, viewpoints, and experiences
- Giving and gracefully accepting constructive feedback
- Accepting responsibility and apologizing to those affected by our mistakes
- Focusing on what is best not just for us as individuals, but for the overall community

Examples of unacceptable behavior include:
- The use of sexualized language or imagery
- Trolling, insulting or derogatory comments
- Public or private harassment
- Publishing others' private information without explicit permission

### Enforcement
Instances of abusive, harassing, or otherwise unacceptable behavior may be reported to the project team. All complaints will be reviewed and investigated promptly and fairly.

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |

## 📄 License
MIT

---
| Redox Port: dtb Scheme | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| Redox Port: memory Scheme | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| Redox Port: user Scheme | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| Redox Port: debug Scheme | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| Redox Port: sys Scheme | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| Redox Port: event Scheme | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| Schemes Registry | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| VFS Offset Support | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| Capability I/O Test | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| WASM Inline Tests | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| VFS Early Init | VFS initialized before self-tests to support snapshot tests and capability I/O. |
<sup>Last updated: June 10, 2026 | Knowledge graph: 10469 nodes, 21249 edges | Token reduction: 75.4x | SMP + APIC: ✅ complete | ACPI/PCI: ✅ complete | Device Model: ✅ complete | PS/2 Mouse: ✅ complete | FAT32: ✅ experimental | Socket Stack: ✅ experimental | eBPF Obsidian-Tier: ✅ complete | NWCC Desktop Demo: ✅ demo | COW Fork + VMA: ✅ complete | Userspace Drivers: ✅ skeleton | 115+ syscalls | 40+ shell commands | Ring 3 + SMEP/SMAP/UMIP: ✅ complete | A-Tier Scalability: ✅ complete | Microkernel Phase 1: ✅ complete | Memory Compression: ✅ experimental | Snapshot Persistence: ✅ experimental | Instant-On Resume: ✅ experimental</sup>
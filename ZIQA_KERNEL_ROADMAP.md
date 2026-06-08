# ZiqaKernel / Axiq-IQ: Status & Engineering Roadmap
*Updated: Sunday, June 7, 2026*

This document provides a comprehensive overview of the current state of the ZiqaKernel, its features from the low-level base to the high-level services, and the roadmap of what has been implemented and what remains for the future.

---

## 🔬 Current State of the Kernel
As of June 2026, **ZiqaKernel** is a sophisticated architectural scaffold and OS research sandbox. It is fully capable of booting on **x86_64 bare metal** (or QEMU), initializing multi-processor cores (SMP), managing complex memory mappings (COW Fork/VMA), and running multi-windowed graphical applications in a Ring 3 userspace environment.

### What can we do with it now?
*   **Run Graphical Apps**: Boot into a graphical desktop (NWCC) with a mouse-driven window manager.
*   **Execute Linux Binaries**: Run static x86_64 Linux ELF binaries via the Linux ABI Plugin — including a custom Doom ELF port compiled from Zig.
*   **Dynamic Tracing**: Instrument the kernel in real-time using the **Obsidian-Tier eBPF VM** with tracepoints and shared maps.
*   **Scalable Multi-Processing**: Utilize multiple CPU cores with fine-grained locking (no Big Kernel Lock).
*   **Full TCP/UDP Lifecycle**: Run both client and server applications with `connect`, `listen`, and `accept` support.
*   **Security Research**: Experiment with **Instant Recursive Capability Revocation** to sever process access system-wide.
*   **Bare-metal Demos**: Run DOOM fire effects, a standalone Doom ELF process (via `int 0x80` syscall forwarding), and graphical Tetris directly from the boot screen.

---

## 🏛️ The Feature Floor Spectrum

### 1. Base Floor (Hardware & Core Memory)
*   **Arch Abstraction**: x86_64 Long Mode, GDT, IDT, TSS, PIC/IOAPIC remapping.
*   **SMP (Symmetric Multi-Processing)**: INIT-SIPI-SIPI AP boot, Per-CPU structs via GS.base, IPI infrastructure (TLB shootdown, Reschedule).
*   **APIC & ACPI**: Full table parsing (MADT, FADT), Local APIC timer calibration (PIT-based).
*   **Memory Management**:
    *   4-Level Paging with per-process address spaces.
    *   **COW Fork**: Copy-on-Write fork with page table cloning.
    *   **VMA System**: Virtual Memory Area management for `mmap`/`brk`.
    *   **Demand Paging**: ELF segments are registered as VMAs and populated on not-present page faults.
    *   **SMEP/SMAP/UMIP**: Hardware-enforced supervisor protection.

### 2. Middle Floor (Drivers, I/O & VFS)
*   **Device Driver Model**: Generic `Driver` trait with PCI auto-probing and `DeviceManager`.
*   **Block & Storage**: `BlockRegistry` with VirtIO-Block (MMIO) and ATA drivers.
*   **Filesystems**:
    *   **VFS**: Virtual Filesystem Switch with mount support.
    *   **ZiqaFS**: Journaling filesystem (Custom).
    *   **FAT32**: Pure-Rust host-interop driver with file write, cluster allocation, file creation, directory creation, rename, and truncate support.
    *   **RamFS & Page Cache**: High-speed memory-backed storage and LRU caching.
*   **Network Stack**: `smoltcp`-based TCP/UDP socket layer with full server/client support, VirtIO-Net drivers.
*   **Input/Output**: PS/2 Mouse (clamped 1080p), PS/2 Keyboard, UART serial, VGA LFB.

### 3. High Floor (Process, Security & Userspace)
*   **Process Management**:
    *   **MLFQ Scheduler**: Multi-Level Feedback Queue with priority boosting.
    *   **Ring 3 Isolation**: Full privilege separation with assembly-hardened syscall trampolines.
    *   **IPC**: Bounded Channels, Signals, Shared Memory (SHM), and `io_uring`.
*   **Security & Capabilities**:
    *   **Capability Space**: Per-process resource tokens (Files, Devices, IPC).
    *   **Recursive Revocation**: Instant severance of delegated capabilities across descendants.
*   **ABI Plugin System**:
    *   **Linux ABI**: 112 unique registered syscalls with 85+ functional implementations.
    *   **WASM ABI**: Experimental WebAssembly runtime integration.
*   **eBPF Obsidian-Tier**: Advanced VM with Maps (Hash/Array/RingBuf), Tail Calls, Bounded Loops, and Tracepoints.
*   **Compositor (NWCC)**: VGA-downsampled text-mode window manager with an application API.

---

## 📈 Engineering Roadmap: Added vs. Missing

### ✅ ADDED (Completed & Hardened)
*   **SMP/APIC/ACPI**: Full multi-core and interrupt infrastructure.
*   **COW Fork & VMA**: Mature memory management for process cloning.
*   **Ring 3 & SMEP/SMAP**: Hardened security boundaries.
*   **eBPF VM (Full Stack)**: Verifier, Maps, Tracepoints, and Helpers.
*   **PCI/Device Model**: Robust driver discovery and registration.
*   **Native `io_uring` & SHM**: Fast-path IPC and I/O.
*   **PS/2 Mouse**: Smooth cursor integration for NWCC.
*   **Recursive Revocation**: Advanced capability security model.
*   **Socket Stack (Full Lifecycle)**: TCP `listen` and `accept` are fully implemented and integrated with the Linux ABI.
*   **Demand Paging Foundation**: Not-present page faults allocate and populate process pages from VMA metadata and stored ELF bytes.
*   **FAT32**: Full read/write, rename, and truncate functionality.
*   **Doom ELF Port**: Standalone Doom ELF binary (`src/zig/doom_port.zig`) compiled for x86_64-linux-musl, embedded in the kernel VFS at `/bin/doom`, and executed as a userspace process via `int 0x80` syscall forwarding (FB_BLIT, GET_TICKS, GET_KEY syscalls).

### 🧪 IN-PROGRESS (Experimental / Needs Maturation)
*   **Userspace Drivers**: DRM driver now supports basic GPU command submission via IPC; VirtIO-Net driver implements full packet processing (RX rings, DMA buffer management, event loops) and command management via native syscalls; ready for network protocol stack integration.
*   **POSIX Completeness**: Signal management is improving (trampoline logic and registration implemented); advanced memory protection polish remains required.
*   **WASM ABI**: Host function dispatcher refactored to be modular and capability-aware; key WASI functions implemented (`fd_read`, `fd_write`, `args_get`, `args_sizes_get`, `proc_exit`); further expansion required for full capability mapping.
*   **Kernel Threading**: `spawn_kthread()` supports generic arguments; `join` and `cancel` semantics are implemented; requires additional work on worker pools and scheduler parity with user processes.
*   **Zig Build Orchestration**: Unified `build.zig` manages the full pipeline (Zig static libs → doom.elf → cargo build → bootimage → QEMU runners). The `build.rs` triggers Zig compilation for freestanding hot-path libs when `zig-hotpaths` feature is enabled.
*   **Zig Hot-paths (blitter)**: Framebuffer blitter (`src/zig/blitter.zig`) with fill_rect, blit_bitmap, scroll_up, clear, doom_fire_step, fireworks_burst — called from Rust via C ABI. Compiled for `x86_64-freestanding-none` with ReleaseFast.

### ❌ MISSING (Not Yet Added / Future)
*   **Production-grade Graphics**: Move from VGA LFB to real DRM/KMS acceleration.
*   **Multi-Architecture**: Support for `aarch64` (ARM64) or `riscv64`.
*   **Production Demand Paging**: Current demand paging is functional but rough; anonymous/file-backed page policy, eviction, swap, and stricter VMA source handling are still missing.
*   **USB Stack**: No support for USB keyboards, mice, or mass storage yet.
*   **Sound Subsystem**: No audio drivers or ABI.
*   **Production Kernel Threading**: Need worker pools and affinity controls.

---

## 🚀 Future Vision (P2 Roadmap)
1.  **Microkernel Maturation**: Move remaining Block, Net, and GPU driver logic into Ring 3 using `DeviceIo` capabilities. Current state is Phase 1: `DeviceIo` syscalls plus DRM/Net userspace driver skeletons, while block and several hardware drivers still initialize in kernel space.
2.  **Native Rust ABI**: Move beyond Linux emulation to a native Ziqa-native ABI optimized for capability security.
3.  **Zig Hot-paths (Expansion)**: Extend Zig usage beyond the blitter to VFS, networking, and filesystem hot-paths. The `build.zig` + `build.rs` infrastructure is in place for integrating more Zig code into the kernel.
4.  **Doom ELF Maturation**: Connect the standalone `doom_port.zig` with `doomgeneric` C source (when imported) for a full playable Doom port. Currently the DG_* exports and syscall layer are ready for linking.
5.  **Self-Hosting**: Port a compiler (Rustc/Zig) and build tools to run natively on ZiqaKernel.

---
*ZiqaKernel is an architectural laboratory for exploring the limits of safety-critical systems.*

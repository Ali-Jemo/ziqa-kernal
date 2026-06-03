# ZiqaKernel / Axiq-IQ: Status & Engineering Roadmap
*Updated: Wednesday, June 3, 2026*

This document provides a comprehensive overview of the current state of the ZiqaKernel, its features from the low-level base to the high-level services, and the roadmap of what has been implemented and what remains for the future.

---

## 🔬 Current State of the Kernel
As of June 2026, **ZiqaKernel** is a sophisticated architectural scaffold and OS research sandbox. It is fully capable of booting on **x86_64 bare metal** (or QEMU), initializing multi-processor cores (SMP), managing complex memory mappings (COW Fork/VMA), and running multi-windowed graphical applications in a Ring 3 userspace environment.

### What can we do with it now?
*   **Run Graphical Apps**: Boot into a graphical desktop (NWCC) with a mouse-driven window manager.
*   **Execute Linux Binaries**: Run static x86_64 Linux ELF binaries via the Linux ABI Plugin.
*   **Dynamic Tracing**: Instrument the kernel in real-time using the **Obsidian-Tier eBPF VM** with tracepoints and shared maps.
*   **Scalable Multi-Processing**: Utilize multiple CPU cores with fine-grained locking (no Big Kernel Lock).
*   **Full TCP/UDP Lifecycle**: Run both client and server applications with `connect`, `listen`, and `accept` support.
*   **Security Research**: Experiment with **Instant Recursive Capability Revocation** to sever process access system-wide.
*   **Bare-metal Demos**: Run DOOM fire effects and graphical Tetris directly from the boot screen.

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
    *   **FAT32**: Pure-Rust host-interop driver with file write, cluster allocation, file creation, and directory creation support.
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
*   **Compositor (NWCC)**: VGA-downsampled text-mode window manager with application API.

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

### 🧪 IN-PROGRESS (Experimental / Skeleton)
*   **Userspace Drivers**: Skeleton DRM and VirtIO-Net drivers (Microkernel phase).
*   **FAT32 Write Path**: File writes, FAT updates, file creation, and directory creation are implemented; disk-backed unlink/rename and long filename support still need work.
*   **POSIX Completeness**: 112 syscalls are registered; focusing on finishing signal management and advanced memory protection.
*   **WASM ABI**: Can load modules but requires better host function coverage.
*   **Kernel Threads**: `spawn_kthread()` and per-CPU scheduling support exist, but kernel thread lifecycle APIs remain limited compared with user processes.

### ❌ MISSING (Not Yet Added / Future)
*   **Production-grade Graphics**: Move from VGA LFB to real DRM/KMS acceleration.
*   **Multi-Architecture**: Support for `aarch64` (ARM64) or `riscv64`.
*   **Production Demand Paging**: Current demand paging is functional but rough; anonymous/file-backed page policy, eviction, swap, and stricter VMA source handling are still missing.
*   **USB Stack**: No support for USB keyboards, mice, or mass storage yet.
*   **Sound Subsystem**: No audio drivers or ABI.
*   **Production Kernel Threading**: Need join/cancel semantics, arguments, worker pools, affinity controls, and clearer kernel/user scheduling parity.

---

## 🚀 Future Vision (P2 Roadmap)
1.  **Microkernel Maturation**: Move remaining Block, Net, and GPU driver logic into Ring 3 using `DeviceIo` capabilities. Current state is Phase 1: `DeviceIo` syscalls plus DRM/Net userspace driver skeletons, while block and several hardware drivers still initialize in kernel space.
2.  **Native Rust ABI**: Move beyond Linux emulation to a native Ziqa-native ABI optimized for capability security.
3.  **Zig Hot-paths**: Expand the use of Zig for performance-critical VFS and Networking hot-paths.
4.  **Self-Hosting**: Port a compiler (Rustc/Zig) and build tools to run natively on ZiqaKernel.

---
*ZiqaKernel is an architectural laboratory for exploring the limits of safety-critical systems.*

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
  <img src="https://img.shields.io/badge/documentation-graph-brightgreen" alt="Knowledge Graph"/>
  <img src="https://img.shields.io/badge/Maintained%3F-yes-green.svg" alt="Maintenance"/>
  <img src="https://img.shields.io/badge/graph_benchmark-75x-brightgreen" alt="75x Token Reduction"/>
</p>

---

## 🔬 Executive Summary
ZiqaKernel is an **experimental OS research sandbox** written in Rust for `x86_64` bare metal — with select hot paths in **Zig**. It acts as a testbed for advanced OS design patterns: **Plugin-based ABI Layer**, **Capability-based Security**, **Hybrid Rust/Zig FFI**, **eBPF verifier + VM**, **io_uring**, **DOOM fire / Tetris demos**, and a staged VGA boot experience.

While the kernel now supports complex applications like DOOM and Tetris, and has advanced features like a VFS and eBPF support, **robust user-space isolation (Ring 3) remains an active development track**. It is not a production-ready OS, but a high-performance architectural laboratory.

---

## 🛠️ Engineering Status & Capabilities (May 2026)

| Component | Maturity | Engineering Assessment |
| :--- | :--- | :--- |
| **Boot & HAL** | **Functional** | Reliable BIOS/UEFI boot. Three-stage VGA boot pipeline with CP437-safe animation. |
| **Scheduler** | **Functional** | Robust MLFQ scheduling with priority boosting, signal delivery, and context switching. |
| **Privilege** | **Work-in-Progress** | Active hardening of Ring 3 transitions, TSS management, and stack isolation. |
| **Syscall ABI** | **Hardened** | 111+ syscalls across fs, process, memory, time, net, misc. |
| **Memory** | **Hardened** | `copy_from_user` with page-table validation, heap profiler, frame allocator. |
| **Hybrid FFI** | **Functional** | Rust → Zig C-ABI blitter for framebuffer ops (DOOM/Tetris demos). |
| **eBPF VM** | **Experimental** | Bytecode verifier (kCFI) + interpreter; tracing and networking use cases. |
| **Shell** | **Feature-rich** | Tab completion, arrow history, 37+ commands, system dashboard. |
| **Graphics** | **Functional** | DOOM fire, Tetris, DRM/KMS driver for future compositor support. |

---

## 🧠 Knowledge Graph Insights
Run `/graphify` to generate an interactive architectural knowledge graph.
- **Key Abstractions**: [`Shell`](src/shell.rs), [`Scheduler`](src/process/scheduler.rs), [`ZiqaFs`](src/fs/ziqafs.rs).
- **Token Efficiency**: 75.4x token reduction per query.
- **Communities**: 122 architectural communities identified.

View the full interactive graph: [graphify-out/graph.html](graphify-out/graph.html)

---

## 🚀 Quick Start
### Build & Run
```bash
make build     # Debug build
make run       # Build + boot image + QEMU
make run-gui   # Run with graphical display (for DOOM/Tetris)
```

### Prerequisites
- **Rust nightly** (via `rust-toolchain.toml`)
- **QEMU**, **Zig** (>= 0.11), **NASM**

---

## 🤝 Contributing
1. Fork and clone.
2. Build with `docker compose run dev`.
3. Open Pull Requests for fixes/features.

---

## 📄 License
MIT

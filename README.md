# ZiqaKernel

An experimental bare-metal operating-system kernel for `x86_64` written in Rust and Zig.

> [!WARNING]
> **Experimental Sandbox:** Not production-ready. Active architecture lab for exploring OS design.

## Project Status

![Rust Nightly](https://img.shields.io/badge/rust-nightly-orange?logo=rust)
![Zig](https://img.shields.io/badge/zig-★-gold?logo=zig)
![x86_64](https://img.shields.io/badge/arch-x86__64-purple)
![License](https://img.shields.io/badge/license-MIT-blue)

## Features

- **Pluggable ABI:** Supports Linux and WASM ABI plugins.
- **Capability-based Security:** Resource access managed via capabilities.
- **Hybrid Architecture:** Rust kernel with Zig hot paths for performance.
- **VFS System:** URL-based scheme resource system (Redox-inspired).

## Getting Started

### Prerequisites
- **Rust:** Nightly toolchain
- **Zig:** Compiler
- **Build Tools:** QEMU, `make`, standard Unix utilities

### Build & Run
```bash
# Core Build
make build     # Debug build
make boot      # Generate bootable image
make run       # Execute in QEMU (serial console)

# Testing & Graphics
make run-gui   # QEMU with GTK display
make test      # Run project-wide cargo tests
make clean     # Clean build artifacts
```

## Development Environment

The repository includes a `Dockerfile` for a consistent build environment:

```bash
docker compose run dev
```

## Code Intelligence

This project uses **GitNexus** and **Graphify** for codebase navigation and impact analysis. If index files appear stale, rebuild them:

```bash
# Update GitNexus index
node .gitnexus/run.cjs analyze

# Update Graphify graph
graphify update .
```

## Cargo Features

The kernel provides several features for customization. The `default` feature set is `full`.

| Feature | Description |
| :--- | :--- |
| `full` | Enables all features (default). |
| `fast-dev` | Lightweight build (shell, vfs). |
| `net` | Enables networking stack (`smoltcp`). |
| `ebpf` | Enables eBPF VM support. |
| `wasm` | Enables WebAssembly ABI support. |
| `games` | Includes demo games. |
| `orbital` | Includes Orbital GUI compositor. |

## Project Structure

| Directory | Description |
| :--- | :--- |
| `src/` | Kernel source (Rust) |
| `gui/` | Orbital compositor (Zig) |
| `userspace/` | Test binaries and utilities |
| `conductor/` | Architecture specs and roadmaps |

## Troubleshooting

- **QEMU locks:** If `make run` fails with write-lock errors, run `make kill-qemu` to clear zombie processes.
- **Disk images:** If disk operations fail, run `make clean` and `make fat-disk` to rebuild the FAT32 disk image.

## Documentation

Detailed documentation is available in the `conductor/` directory:

- [Architecture Specs](conductor/docs/ARCHITECTURE_TARGET.md)
- [Syscall ABI](conductor/SYSCALLS.md)
- [Project Roadmap](conductor/ZIQA_KERNEL_ROADMAP.md)

## Contributing

1. **Explore:** Review architecture and design in `conductor/`.
2. **Build:** Run `make build` and `make test` to ensure environment readiness.
3. **Verify:** Use `make zig-check` for Zig hot paths and run GitNexus impact analysis before any changes.
4. **Submit:** Open a pull request for review.

## License

MIT

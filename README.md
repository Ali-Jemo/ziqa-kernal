# ZiqaKernel

An experimental bare-metal operating-system kernel for `x86_64` written in Rust and Zig.

> [!WARNING]
> **Experimental Sandbox:** Not production-ready. Active architecture lab for exploring OS design.

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

## Project Structure

| Directory | Description |
| :--- | :--- |
| `src/` | Kernel source (Rust) |
| `gui/` | Orbital compositor (Zig) |
| `userspace/` | Test binaries and utilities |
| `conductor/` | Architecture specs and roadmaps |

## Documentation

Detailed documentation is available in the `conductor/` directory:

- [Architecture Specs](conductor/docs/ARCHITECTURE_TARGET.md)
- [Syscall ABI](conductor/SYSCALLS.md)
- [Project Roadmap](conductor/ZIQA_KERNEL_ROADMAP.md)

## Contributing

1. Check existing documentation in `conductor/`.
2. Follow established project conventions in `src/`.
3. Submit a pull request for review.

## License

MIT

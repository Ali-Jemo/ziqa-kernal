# ZiqaKernel

An experimental bare-metal operating-system kernel for `x86_64` written in Rust and Zig.

> [!WARNING]
> **Experimental Sandbox:** Not production-ready. Active architecture lab for exploring OS design.

## Features

- Pluggable ABI (Linux/WASM)
- Capability-based security
- Hybrid Rust/Zig components
- VFS with scheme resource system

## Getting Started

### Prerequisites
- Rust nightly toolchain
- Zig compiler
- QEMU

### Build & Run
```bash
# Core
make build     # Debug build
make boot      # Build + bootimage
make run       # QEMU with serial stdio

# Graphics & Testing
make run-gui   # QEMU with GTK display
make test      # Cargo tests
make clean     # Remove build artifacts
```

## Project Structure

- `src/`: Kernel source (Rust)
- `gui/`: Orbital compositor (Zig)
- `userspace/`: Test binaries
- `conductor/`: Documentation & Specs

## Documentation

See `conductor/` for architecture details, syscall ABI, and roadmap.

## License

MIT

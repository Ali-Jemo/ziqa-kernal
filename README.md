# ZiqaKernel

An experimental bare-metal operating-system kernel for `x86_64` written in Rust and Zig.

## Features

- Pluggable ABI (Linux/WASM)
- Capability-based security
- Hybrid Rust/Zig components
- VFS with scheme resource system

## Project Structure

- `src/`: Kernel source (Rust)
- `gui/`: Orbital compositor (Zig)
- `userspace/`: Test binaries
- `conductor/`: Documentation & Specs

## Build & Run

Requires Rust nightly and Zig.

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

## Documentation

See `conductor/` for architecture details, syscall ABI, and roadmap.

## License

MIT

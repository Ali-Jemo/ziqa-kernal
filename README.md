# ZiqaKernel

An experimental bare-metal operating-system kernel for `x86_64` written in Rust and Zig.

## Features

- Pluggable ABI (Linux/WASM)
- Capability-based security
- Hybrid Rust/Zig components
- VFS with scheme resource system

## Build and Run

Requires Rust nightly and Zig.

```bash
make build   # Build
make boot    # Build + bootimage
make run     # Run in QEMU
```

## License

MIT

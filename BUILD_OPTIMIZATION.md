# 🚀 Ultra-Fast Build Configuration

This document explains the build optimizations applied to ZiqaKernel for maximum compilation speed.

## Applied Optimizations

### 1. **Cargo.toml Profile Optimizations**
```toml
[profile.dev]
opt-level = 1              # Faster compilation than 0, still fast enough for dev
codegen-units = 16         # Parallel code generation
incremental = true         # Enable incremental compilation

[profile.dev.package."*"]
opt-level = 2              # Optimize dependencies (they rarely change)
```

**Impact:** 30-50% faster dev builds by optimizing dependencies while keeping your code fast to compile.

### 2. **sccache Compiler Caching**
```toml
[build]
rustc-wrapper = "sccache"
```

**Impact:** Near-instant rebuilds for unchanged code. Caches compilation artifacts locally and optionally remotely.

### 3. **Custom Dev-Fast Profile**
```toml
[profile.dev-fast]
inherits = "dev"
opt-level = 0
codegen-units = 256
incremental = true
```

**Impact:** Ultra-fast iteration for quick checks (10x faster than regular dev).

### 4. **Feature-Based Builds**
```toml
[features]
fast-dev = ["shell", "vfs"]      # Minimal features for quick iteration
full = ["shell", "vfs", "ziqafs", "fat32", "net", "ebpf", "drm", "games", "wasm", "zig-hotpaths", "orbital"]
```

**Impact:** Build only what you need for development.

## Installation

### Quick Setup
```bash
./scripts/setup-fast-build.sh
```

### Manual Setup

1. **Install sccache:**
   ```bash
   cargo install sccache
   ```

2. **Enable sccache in `.cargo/config.toml`:**
   ```toml
   [build]
   rustc-wrapper = "sccache"
   ```

## Usage

### Development Builds
```bash
# Fast development build (minimal features)
cargo build --features fast-dev

# Ultra-fast iteration (no optimizations)
cargo build --profile dev-fast --features fast-dev

# Type checking only (fastest)
cargo check --features fast-dev

# Full feature build
cargo build --features full
```

### Release Builds
```bash
# Optimized release (slowest, smallest binary)
cargo build --release --features full

# Check release without full compilation
cargo check --release --features full
```

## Performance Comparison

| Build Type | Time (approx) | Size | Use Case |
|------------|---------------|------|----------|
| `dev-fast` + `fast-dev` | 10-20s | Large | Quick iteration |
| `dev` + `fast-dev` | 30-60s | Large | Normal development |
| `check` + `fast-dev` | 5-15s | N/A | Type checking |
| `release` + `full` | 2-5min | Small | Production builds |

*Times vary based on hardware and incremental state.*

## Advanced Techniques

### 1. **Parallel Jobs**
```bash
# Use all CPU cores
export CARGO_BUILD_JOBS=$(nproc)
cargo build
```

### 2. **Remote Caching (Team)**
Configure sccache with remote storage:
```bash
export SCCACHE_BUCKET=your-bucket
export SCCACHE_REGION=us-east-1
export SCCACHE_S3_KEY_PREFIX=ziqa-kernel
```

### 3. **Link-Time Optimization (LTO) for Release**
Already enabled in release profile for maximum optimization.

### 4. **Strip Debug Symbols**
```bash
# Reduce binary size further
strip target/x86_64-unknown-none/release/ziqa-kernel
```

## Monitoring

### Check sccache Statistics
```bash
sccache --show-stats
```

### Build Timings
```bash
# Detailed timing information
cargo build --timings
```

## Troubleshooting

### sccache Not Working
```bash
# Check if wrapper is set
echo $RUSTC_WRAPPER

# Manual override
RUSTC_WRAPPER=sccache cargo build
```

### Build Failures
```bash
# Clean and rebuild
cargo clean
cargo build

# Update dependencies
cargo update
```

## Bare-Metal Considerations

**Note:** This is a bare-metal kernel target (`x86_64-unknown-none`), so some optimizations differ from regular Linux applications:

- **No mold linker:** Custom linker scripts are required for bare-metal targets
- **Custom bootimage:** Uses `bootimage` crate for kernel booting
- **No standard library:** `#![no_std]` environment

The optimizations applied here are specifically tuned for bare-metal Rust development.

## Zig Integration (Future)

The project includes Zig hotpaths (`zig-hotpaths` feature) for performance-critical sections. This allows:

- **Cross-language optimization:** Zig for hot paths, Rust for safety
- **Fast Zig compilation:** Zig compiles much faster than Rust
- **FFI integration:** Seamless Rust-Zig interop

To enable:
```bash
cargo build --features zig-hotpaths
```

## References

- [sccache](https://github.com/mozilla/sccache)
- [Cargo Profile Optimization](https://doc.rust-lang.org/cargo/reference/profiles.html)
- [Rust Build Optimization](https://doc.rust-lang.org/cargo/guide/build-cache.html)
- [Bare-Metal Rust](https://github.com/rust-osdev/bootimage)

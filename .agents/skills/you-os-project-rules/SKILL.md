---
name: you-os-project-rules
description: Defines the architecture, project-specific rules, and key subsystems for ZiqaKernel. Use this to understand the project structure.
license: MIT
metadata:
  project: ZiqaKernel
  subsystem: general
---

# ZiqaKernel Project Rules

## Key Architectural Rules
1. **FFI Strategy**: Zig hotpath FFI components in `build.rs`. Use `src/zig_ffi.rs` to interface.
2. **Resource System**: Scheme-based (URL-like) resources (e.g., `tcp://`, `pipe://`).
3. **IPC**: Fixed-size ring-buffer channels + shared memory.
4. **Memory**: LZ4 compression tiered swap.
5. **Compositor**: Orbital (Zig).

## Directory Boundaries
- `src/abi/`: ABI plugins (Linux, Redox, WASM).
- `src/scheme/`: Resource URL handlers.
- `gui/`: Orbital/Zig compositor.
- `third_party/rmm/`: Frame allocator.

## Debugging Constraints
- **Serial**: All boot tests output via serial.
- **Panic**: Check `src/tests.rs` for boot-time test runner.
- **Userspace Drivers**: Use `make userspace-test` for C tests.

## Ponytail Compliance
Always prefer:
- Standard library over custom primitives
- Minimal FFI surface
- Direct resource usage over complex wrappers

ponytail: This file acts as the project truth. Update it immediately when architecture changes to prevent stale agent knowledge.

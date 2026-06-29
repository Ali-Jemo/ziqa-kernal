# YOU OS / ZiqaKernel Agent Rules

This repository is a Rust `no_std` operating system kernel project.

For any kernel, GUI, scheduler, ELF, IPC, syscall, framebuffer, boot, QEMU, serial-log, or userspace task:

1. Use the `you-os-kernel-accelerator` skill when available.
2. Use Ponytail policy: smallest safe change only.
3. Do not patch before reading the exact file.
4. Do not guess file contents.
5. Search error memory before debugging if available.
6. Use Graphify/GitNexus for relationships or impact analysis when unclear.
7. Prefer serial logs over visual guessing.
8. Do not treat GUI freeze as kernel freeze without timer/PID evidence.
9. Do not touch unrelated subsystems.
10. After a successful fix, record the incident.

Default verification style:

- Use the smallest relevant check.
- Prefer diagnostic mode before full boot.
- Report exact files changed.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **ziqa-kernal**. Use GitNexus MCP/CLI tools to understand code, assess impact, and navigate safely.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `impact({target: "symbolName", direction: "upstream"})` and report blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows. For regression review, compare against the default branch: `detect_changes({scope: "compare", base_ref: "main"})`.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `query({query: "concept"})` to find execution flows instead of grepping.
- When you need full context on a symbol — callers, callees, and flows — use `context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running GitNexus impact analysis.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use a graph-aware rename.
- NEVER commit changes without running `detect_changes()`.

## CLI

```bash
node .gitnexus/run.cjs status
node .gitnexus/run.cjs analyze
node .gitnexus/run.cjs list
node .gitnexus/run.cjs wiki
node .gitnexus/run.cjs clean
```

<!-- gitnexus:end -->

## graphify

This project has a knowledge graph at `graphify-out/` with god nodes, community structure, and cross-file relationships.

Rules:

- For codebase questions, first run `graphify query "<question>"` when `graphify-out/graph.json` exists.
- Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts.
- Dirty `graphify-out/` files are expected after hooks or incremental updates; dirty graph files are not a reason to skip graphify.
- If `graphify-out/wiki/index.md` exists, use it for broad navigation instead of raw source browsing.
- Read `graphify-out/GRAPH_REPORT.md` only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current.

## Project Overview

ZiqaKernel is an experimental OS kernel for x86_64 (with aarch64/riscv64/i686 support) written in Rust nightly with Zig hotpath FFI components.

Features include:

- Pluggable ABI architecture (Linux ELF, Redox ELF, WASM)
- Capability-based security
- Redox-inspired scheme resource system (`tcp://`, `pipe://`, etc.)
- CFS-style DWRR scheduler
- IPC via fixed-size ring-buffer channels + shared memory
- Hierarchical VFS with mount points
- smoltcp-based networking
- PCI device manager with VirtIO/ATA/NVMe drivers
- LZ4 memory compression tiered swap
- Verified eBPF VM
- Graphical compositor with SHM-backed surfaces

Boot flow:

```text
bootloader → _start (linker.ld) → kernel_main (main.rs) → init.rs → shell
                          ↓
                abi::plugin::handle_syscall ← userspace
                          ↓
                process scheduler ↔ VFS ↔ drivers
```

## Key Directories

|Directory|Purpose|
|---|---|
|`src/`|Kernel source (Rust)|
|`src/arch/x86_64/`|x86_64-specific code|
|`src/drivers/`|Hardware drivers|
|`src/fs/`|Filesystems|
|`src/memory/`|Memory management|
|`src/process/`|Process/thread management|
|`src/scheme/`|URL-based resource schemes|
|`gui/`|Orbital compositor (Zig)|
|`userspace/`|Userspace test binaries|
|`third_party/rmm/`|Frame allocator library|

## Development Commands

```bash
cargo build
cargo build --features fast-dev
cargo build --release
make run
make run-gui
make boot
make test
make fast
cargo build --features zig-hotpaths
```

## Cargo Features

- `skip-self-tests`
- `orbital`
- `net`
- `wasm`
- `perf-benchmarks`
- `userspace-drivers-test`

## Testing & QA

```bash
cargo test
make run
make userspace-test
```

Test files:

- `src/tests.rs` — main self-test runner
- `src/tests_fix.rs` — eBPF, snapshot tests
- `src/tests_net.rs` — network tests

## Code Conventions

- Entry point: `src/main.rs` → `init()` → `shell::start()`
- Error handling: `Result<T, Error>` with module-local `Error` enums
- Async: no async/await; cooperative multitasking via scheduler
- FFI: Zig hotpaths in `build.rs` (`blitter`, `kernel_ops`)
- Testing: boot-time tests via `test!()` macro, unit tests via `#[cfg(test)]`

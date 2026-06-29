# ZiqaKernel

<div align="center">
  <img src="assets/logo.svg" alt="ZiqaKernel Logo" width="250"/>
  <h1>ZiqaKernel</h1>
  <p><strong>Rust + Zig experimental OS research playground for x86_64</strong></p>
</div>

<p align="center">
  <img src="https://img.shields.io/badge/rust-nightly-orange?logo=rust" alt="Rust Nightly"/>
  <img src="https://img.shields.io/badge/zig-★-gold?logo=zig" alt="Zig"/>
  <img src="https://img.shields.io/badge/arch-x86__64-purple" alt="x86_64"/>
  <img src="https://img.shields.io/badge/status-experimental-yellow" alt="Status"/>
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="License"/>
</p>

ZiqaKernel is an experimental bare-metal operating-system kernel for `x86_64`. It is a research sandbox for OS architecture, drivers, capability-style resource access, hybrid Rust/Zig hot paths, filesystems, ABI plugins, graphics/compositor experiments, and kernel-level tooling.

It is **not production-ready**. Treat it as an active architecture lab: useful for exploring boot flow, paging, scheduling, device drivers, VFS/schemes, ABI plugins, graphics IPC, and kernel experiments, but not as a stable user environment.

---

## GitNexus Project Map

| Metric | Value |
| --- | ---: |
| Indexed files | 280 |
| Symbols / graph nodes | 5,976 |
| Relationships / edges | 13,792 |
| Communities | 192 |
| Execution flows | 288 |
| Index timestamp | 2026-06-28 |

GitNexus currently highlights these project communities as major navigation points:

| Community | Cohesion | Symbols |
| --- | ---: | ---: |
| Drivers | 0.800+ | 125+ |
| X86_64 | 0.795+ | 23+ |
| Userspace | 0.910+ | 29+ |
| Scheme | 0.850+ | 125+ |
| Abi | 0.877+ | 43+ |
| Kernel Core Types | 0.736+ | 28+ |
| Kernel Scheduler | 0.731+ | 23+ |
| Kernel Shell | 0.763+ | 21+ |
| Orbital Compositor | 0.850+ | 20+ |
| Window Manager Demo | 0.800+ | 15+ |
| VGA Display Driver | 0.795+ | 18+ |
| Bash-5.2 | 0.900+ | 500+ |

Relevant GitNexus execution flows for orienting around the kernel include:

- `kernel_main` → early init, PCI config, device printing, and startup.
- `init` → BSP/APIC setup, LAPIC read/write, memory allocator init, scheduler init, driver registration.
- `compositor_main` → surface creation, dirty-rectangle union, and surface handling.
- `demo_client_main` → compositor client messaging.
- `you_desktop_main` → desktop window management and rendering.
- `init_services` → RAM filesystem mounts, FAT32/ZiqaFS mounting, and init capability setup.

If the index becomes stale, rebuild it from the repo root:

```bash
node .gitnexus/run.cjs analyze
```

For architecture questions, prefer GitNexus first:

```text
impact({target: "symbolName", direction: "upstream", repo: "ziqa-kernal"})
context({name: "symbolName", repo: "ziqa-kernal"})
query({query: "compositor IPC page fault scheduler", repo: "ziqa-kernal"})
detect_changes({scope: "compare", base_ref: "main"})
```

The older Graphify artifacts are also available under [`graphify-out/`](graphify-out/), especially [`GRAPH_REPORT.md`](graphify-out/GRAPH_REPORT.md): 4,162 nodes, 11,459 edges, 263 communities.

---

## Current Status

| Area | Status | Notes |
| Boot / HAL | Functional / optimized | 50ms to shell (QEMU/KVM), 42 self-tests (41 pass), spin-based APIC calibration, no-debug boot path. |
| SMP / APIC | Experimental | BSP/AP setup, per-CPU data via GS.base, IPI reschedule/TLB shootdown, APIC timer calibration. |
| ACPI / PCI | Experimental | ACPI table parsing, PCI enumeration, BAR decoding, driver matching. |
| Memory | Experimental | Frame allocator, kernel mapper, heap, paging, per-process page tables, COW fork, VMA support. |
| Scheduler / processes | Experimental | Process table, ready queue, kernel threads, ELF/native process launch, signal handling, process snapshot save/restore with instant-on resume. |
| VFS / schemes | Experimental | VFS mount layer, scheme registry, Redox-style schemes, RAM mounts, FAT32 read-only mount, ZiqaFS. |
| Drivers | Experimental | VirtIO block/GPU, ATA/AHCI, NVMe, xHCI, PS/2 mouse, keyboard, framebuffer, audio, DRM. |
| ABI / syscalls | Experimental | Linux ABI plugin, POSIX syscall surface, WASM ABI behind feature flags, eBPF VM/verifier, shell syscall inspection/probes. |
| Graphics / compositor | Demo / experimental | VirtIO GPU or BGA framebuffer, SHM-backed surfaces, dirty rectangles, compositor IPC channel, demo client, NWM text desktop mirrored into the linear framebuffer. |
| Userspace drivers | Skeleton | DRM and VirtIO-Net userspace driver experiments behind feature flags. |
| Network | Experimental | smoltcp-backed TCP/UDP socket experiments behind `net` feature. |
| Memory compression | Experimental | LZ4 page compression, compressed page store, page-fault decompression, daemon/status hooks. |
| Snapshots | Functional / experimental | Process snapshot save/load/resume through FAT32-backed storage; instant-on resume restores full process state (registers, VMAs, FDs) at boot. |
| Shell / demos | Functional / experimental | Foreground-safe shell, syscall inspection tools, BusyBox, DOOM/Tetris/NWM demo clients behind `games` feature, built-in utilities. QEMU GUI mode uses raw terminal serial input plus PS/2 polling. |

The current boot path reaches an interactive shell in **~50ms** (QEMU/KVM):

1. **Bootloader entry** calls [`kernel_main`](src/main.rs).
2. [`ziqa_kernel::init`](src/init.rs) — memory, scheduler, PCI, drivers, APIC/SMP. Clean summary prints.
3. [`init_subsystems`](src/main.rs) — network stack, PS/2 keyboard/mouse, and self-test gate.
4. [`init_services`](src/main.rs) — RAM mounts, FAT32/ZiqaFS disk mount at `/fat`, block device capability grants, and startup assets.
5. **Display** — VirtIO GPU framebuffer when available; otherwise BGA framebuffer console remains active for QEMU GTK.
6. **Startup** — Mouse server, test ELF, keyboard driver, optional Orbital GUI, snapshot instant-on resume.
7. **Shell** — prompt ready at `uptime=40–60ms`; foreground shell owns input without timer preemption.

When a FAT32 disk is attached as virtio-blk, the kernel mounts it at `/fat` and
exposes files via VFS. Orbital GUI can be loaded from `/fat/bin/orbital.elf` on disk
or from the embedded compressed binary (behind the `orbital` feature flag).
Process snapshots enable instant-on resume: saved process state is restored from
FAT32-backed snapshot files at boot, skipping full re-initialization.

## Feature Map

### Core Kernel

- x86_64 paging, frame allocation, heap initialization, and kernel virtual mapping.
- Per-process page tables, COW fork, VMA-based memory regions, and page-fault handling.
- Process table, ready queue, kernel threads, ELF/native process launch, signal handling, and process snapshot save/restore with instant-on resume.
- Interrupt/exception handlers for page fault, double fault, GPF, timer, keyboard, and syscall paths.
- SMP/APIC experiments with per-CPU data, IPIs, APIC timer, and BSP/AP coordination.

### Drivers and Hardware

- PCI enumeration and a generic `Driver` trait with PCI match/init lifecycle.
- VirtIO block, legacy VirtIO block, VirtIO GPU, ATA/AHCI, NVMe, xHCI, PS/2 mouse, keyboard, framebuffer, audio, and DRM driver modules.
- Bochs Graphics Adapter fallback path for framebuffer display.
- Block registry abstraction for filesystem and block-device consumers.

### Filesystems and Schemes

- VFS mount layer with offset-aware file access.
- Scheme registry and Redox-style scheme ports:
  - `dtb` for Device Tree Blob access.
  - `memory` for physical memory access.
  - `user` for userspace scheme hosting experiments.
  - `debug` for serial/debug console behavior.
  - `sys` for process/scheme/CPU/uptime information.
  - `event` for event readiness and epoll-style integration.
- FAT32 read-only mounting under `/fat`.
- ZiqaFS experimental filesystem mounting under `/disk`.

### ABI, Security, and Tooling

- Linux ABI plugin with syscall dispatch and POSIX-facing helpers.
- WASM ABI behind the `wasm` feature.
- eBPF verifier/VM experiments with maps, helpers, bounded loops, tracepoints, and tail calls.
- Capability-style resource checks and DeviceIo grants for userspace driver experiments.
- Performance utilities using RDTSC and heap profiling hooks.


### Shell and Runtime Diagnostics

- Foreground shell owns serial/PS2 input while a command line is active; timer-driven scheduler preemption stays disabled in the shell so boot-spawned demos/drivers cannot steal the input loop after the first timer slice.
- `make run-gui` starts QEMU with a TCP-backed serial port, attaches that serial stream to the invoking terminal through `socat`, and restores terminal state on exit. This avoids QEMU/GTK owning stdio directly while still preserving byte-at-a-time shell input.
- PS/2 keyboard input is polled from `read_stdin()` with IRQ1 disabled, avoiding missed or duplicated GUI keystrokes in QEMU.
- `syscalls [filter]` lists the curated native syscall table by name, number, category, arguments, and safety.
- `syscall <name|nr>` safely probes side-effect-free syscall state such as PID, parent PID, uptime ticks, current signal mask, and GPU channel ID. Unsafe process, IPC mutation, network, and framebuffer calls are listed but not invoked by the shell probe.

### Graphics and Userspace Experiments

- Kernel-mode compositor with SHM-backed surfaces, dirty rectangles, damage tracking, and compositing when a working VirtIO GPU framebuffer is present.
- Compositor IPC protocol documented in [`conductor/docs/COMPOSITOR_IPC.md`](conductor/docs/COMPOSITOR_IPC.md).
- NWM demo (`nwm-test`) renders an 80×25 text desktop through VGA memory and mirrors dirty cells into the active linear framebuffer so QEMU GTK shows the desktop on BGA/VirtIO display paths.
- NWM supports desktop icons, taskbar restore/focus, start-menu launching, window maximize/restore, contextual mouse cursor shapes, and the Snake/Cube/System monitor demos.
- Demo client and userspace display server experiments.
- Zig blitter hot paths for framebuffer operations.
- DOOM/Tetris/demo binaries behind the `games` feature.
- Orbital GUI compositor loaded from FAT32 disk or embedded compressed binary, with full capability grants.

---

## Build and Run

### Quick Commands

```bash
make build     # Debug build
make boot      # Build + bootimage
make run       # QEMU with serial stdio and virtio-blk disk
make run-gui   # QEMU with GTK display + socat-attached serial shell
make test      # Cargo tests
make clean     # Remove build artifacts
make zig-check # Verify Zig blitter compiles independently
```

`make run-gui` opens a GTK display and keeps the serial shell attached to the invoking terminal through `socat`. Type shell commands in the terminal. The runner sets raw terminal mode for byte-at-a-time input while `socat` is attached and restores your terminal when QEMU exits; press `Ctrl-]` to detach and stop the runner. The default GTK display disables OpenGL (`gtk,gl=off`) to avoid host-driver stalls; set `ZIQA_QEMU_DISPLAY=gtk,gl=on` if you explicitly want GL. If VirtIO GPU setup fails, the kernel keeps the BGA framebuffer console active so the GTK window continues mirroring shell output.

Create a host-editable FAT32 development disk:

```bash
make fat-disk
```

`fat-disk` requires host tools such as `parted`, `mkfs.vfat`/`mkfs.fat`, loop-device support, and optionally `mtools`.

### Feature Flags

From [`Cargo.toml`](Cargo.toml):

```text
default = ["full"]
full = ["shell", "vfs", "ziqafs", "fat32", "net", "ebpf", "drm", "games", "wasm", "zig-hotpaths"]
fast-dev = ["shell", "vfs"]
shell = ["vfs"]
vfs = []
ziqafs = ["vfs"]
fat32 = ["vfs"]
net = ["dep:smoltcp"]
ebpf = []
drm = []
games = []
wasm = []
zig-hotpaths = []
userspace-drivers-test = []
perf-benchmarks = []
```

Use `fast-dev` when you only need shell/VFS iteration:

```bash
cargo build --no-default-features --features fast-dev --bin ziqa-kernel
```

### Docker Development Environment

The repository includes a [`Dockerfile`](Dockerfile) and [`docker-compose.yml`](docker-compose.yml):

```bash
docker compose run dev
```

---

## Project Layout

| Path | Purpose |
| --- | --- |
| [`src/main.rs`](src/main.rs) | Bootloader entry, subsystem/service initialization, startup, and shell handoff. |
| [`src/init.rs`](src/init.rs) | Early kernel init, memory setup, scheduler init, driver registration, APIC/SMP, CPU features. |
| [`src/arch/x86_64/`](src/arch/x86_64) | GDT/IDT/PIC/APIC/SMP, paging, interrupts, syscalls, CPU features, per-CPU state. |
| [`src/process/`](src/process) | Process table, scheduler, signal handling, VMA, snapshot experiments. |
| [`src/memory/`](src/memory) | Frame allocator, heap, paging, compression subsystem. |
| [`src/fs/`](src/fs) | VFS, ZiqaFS, FAT32, RAM filesystem. |
| [`src/drivers/`](src/drivers) | PCI, VirtIO, ATA/AHCI, NVMe, xHCI, keyboard, mouse, framebuffer, DRM, audio. |
| [`src/userspace/`](src/userspace) | Compositor, demo client, userspace DRM/net/display driver experiments. |
| [`src/scheme/`](src/scheme) | Scheme registry and scheme implementations. |
| [`src/abi/`](src/abi) | ABI plugins, syscall dispatch, Linux/WASM ABI surfaces. |
| [`src/ebpf/`](src/ebpf) | eBPF verifier, VM, maps, helpers, tracepoints. |
| [`src/shell.rs`](src/shell.rs) | Shell parser, command registry, builtins, job control, syscall table/probe commands. |
| [`conductor/`](conductor/) | Documentation: changelog, roadmap, architecture, syscall ABI, compositor IPC. |
| [`gui/`](gui/) | Zig-based GUI stack: youcanvas (drawing), youui (widgets), youclient (compositor IPC bridge). |
| [`third_party/rmm/`](third_party/rmm/) | Redox Memory Management crate for hardware memory abstractions. |
| [`assets/`](assets/) | Diagrams and embedded boot/test binaries. |

---

## Documentation Map

| Document | Use |
| --- | --- |
| [`conductor/docs/ARCHITECTURE_TARGET.md`](conductor/docs/ARCHITECTURE_TARGET.md) | Target spec, linker script, bootimage config, toolchain, build script, graphics/compositor architecture. |
| [`conductor/docs/IMPLEMENTATION_ROADMAP.md`](conductor/docs/IMPLEMENTATION_ROADMAP.md) | Memory compression implementation status and next steps. |
| [`conductor/docs/COMPOSITOR_IPC.md`](conductor/docs/COMPOSITOR_IPC.md) | Compositor IPC opcodes and input-event channel protocol. |
| [`conductor/docs/CHANGELOG.md`](conductor/docs/CHANGELOG.md) | Fix history and feature additions. |
| [`conductor/SYSCALLS.md`](conductor/SYSCALLS.md) | Syscall ABI contract and register convention. |
| [`conductor/ZIQA_KERNEL_ROADMAP.md`](conductor/ZIQA_KERNEL_ROADMAP.md) | Status and roadmap summary. |

---

## Engineering Notes

### Current Strengths

- Clear separation between early init, driver registration, subsystem init, services, display setup, verification, and shell startup.
- GitNexus index provides a navigable graph for high-risk symbols and execution flows.
- Driver registry and block registry decouple filesystems from concrete hardware drivers.
- VFS/scheme layer gives a consistent path for kernel services and userspace-facing experiments.
- Graphics path has both VirtIO GPU and BGA fallback, with an IPC protocol for compositor clients.

### Watch Areas

- Most components remain experimental and feature-gated; do not assume end-to-end maturity from a module's existence.
- The GitNexus index includes vendored/external trees, so narrow queries by `src/` or a known symbol path when possible.
- The codebase has many weakly connected nodes; documentation and graph coverage are useful but not complete.
- Before changing a function, class, method, or exported symbol, run GitNexus impact analysis and inspect callers.
- Before committing, run GitNexus change detection to verify the affected symbols and flows.

---

## Contributing

1. Read the relevant docs first: architecture target, implementation roadmap, and GitNexus context.
2. For code changes, run GitNexus impact analysis before editing a symbol.
3. Keep changes narrow: update the implementation, affected tests, and docs together.
4. Prefer existing conventions over new abstractions.
5. Run the smallest useful check for the touched area:
   - `make build` for kernel build changes.
   - `make test` for library/test changes.
   - `make zig-check` for Zig blitter changes.
   - `make run-gui` only when exercising graphics/demo behavior.
6. Before committing, run:

```bash
node .gitnexus/run.cjs analyze
# Then use GitNexus detect_changes against main before final review.
```

---
## License

MIT

_Last updated: 2026-06-28. GitNexus index: 5,976 nodes, 13,792 edges, 192 clusters, 288 flows. Graphify artifacts available under graphify-out/.

---

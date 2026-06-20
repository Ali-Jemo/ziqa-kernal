# 2026-06-20 — QEMU input ownership and BGA console fallback

| Issue | Fix |
| :--- | :--- |
| **Shell freezes after first timer slice** | The in-kernel shell now keeps timer-driven scheduler preemption disabled while it owns the command line, preventing boot-spawned demos/drivers from stealing the input loop after the prompt appears. |
| **Host terminal echo looked like kernel input** | `run-gui.sh` now saves/restores host terminal settings and forces raw noncanonical mode before launching QEMU with `-serial stdio`. |
| **QEMU GTK keyboard duplication/misses** | PS/2 keyboard input is polled from `read_stdin()` with IRQ1 disabled, avoiding ISR/poll double-consumption and missed GUI keystrokes. |
| **Display handoff froze BGA console** | VirtIO GPU display initialization now reports success/failure; the compositor only takes over a working VirtIO framebuffer, and BGA keeps the framebuffer console active. |

| Feature | Description |
| :--- | :--- |
| **Foreground shell input ownership** | Serial and PS/2 input remain responsive even after delayed typing at the shell prompt. Verified with delayed serial `help` and QEMU monitor `sendkey h-e-l-p-ret`. |
| **Raw QEMU GUI runner** | `make run-gui` launches GTK display plus raw terminal serial shell and restores host terminal state on exit. |

---

# 2026-06-17 — QEMU GUI fixes, Orbital from disk, snapshot resume

| Issue | Fix |
| :--- | :--- |
| **QEMU GUI terminal input** | Historical note: the GUI runner was iterated from `-nographic`/TCP serial toward foreground `-serial stdio` with explicit raw terminal handling; see the 2026-06-20 entry for the current behavior. |
| **Orbital GUI from disk** | Orbital compositor loaded from `/fat/bin/orbital.elf` on FAT32 disk instead of embedded binary. Full capability grants (File, Memory, DeviceIo, IpcChannel) for Orbital process. |
| **Keyboard driver ABI detection** | Orbital gets Redox ABI by name; keyboard driver ABI detection improved for compatibility. |
| **Shell spawnelf reads entire file** | Fixed shell `spawnelf` command to read entire ELF file, not just first 64KB. |
| **Orbital ELF spawn OOM** | Fixed 37MB allocation failure when spawning orbital.elf; processes now get CPU time. |
| **Process snapshot instant-on** | Process snapshot save/restore through FAT32-backed storage. Full process state (registers, VMAs, FDs) serialized and restored at boot for instant-on resume. |
| **Makefile cleanup** | Expanded gitignore, removed tracked temporary artifacts, documentation audit notes. |

| Feature | Description |
| :--- | :--- |
| **Process Snapshot (v1)** | Binary snapshot format with ZSNP magic: serializes CPU state, VMAs with page data, FD table, and metadata. Restored at boot from FAT32-backed storage. |
| **QEMU GUI Mode** | `make run-gui` keeps terminal keyboard input usable while a GTK display is open; current runner behavior is documented in the 2026-06-20 entry. |
| **Orbital from Disk** | Orbital compositor loaded from FAT32 disk or embedded compressed binary, with full capability grants. |

---

# 2026-06-13 — Boot-time optimization (50ms to shell)

| Issue | Fix |
| :--- | :--- |
| **Slow boot (~50+ debug prints)** | Removed ~70% of debug `println!` calls from boot path: scheduler spawn_elf (5x per ELF), init step logging (20→3 summary lines), per-device PCI scan, device-manager match loops. Serial I/O reduced from ~50+ lines to ~15. |
| **APIC timer calibration bug (0-ticks → 12.5kHz flood)** | `calibrate_timer(5)` computed `5 × 100 / 1000 = 0` PIT ticks → garbage calibration count → timer fires at insane rate, CPU floods on ISRs, tests hang. Replaced PIT-based wait (which also deadlocks `TIMER.lock()` with timer ISR) with spin-based calibration producing proper ~73M count at 100Hz. |
| **Self-tests held `without_interrupts`** | Removed `interrupts::without_interrupts()` wrapper from `tests.rs::run_all()`. Timer ISR now fires during tests, tick counter advances, no test-side deadlock. |
| **37MB `orbital.elf` embedded by default** | Gated `orbital.elf` behind `orbital` Cargo feature. Without this, kernel binary was 76MB and bootloader triple-faulted. |

| Optimization | Before | After |
| :--- | ---: | ---: |
| Boot to shell | Seconds (or panic) | **~50ms** |
| Self-tests | 0/42 (hung in calibration) | **41/42 pass** |
| PCI scan output | 7+ lines per device | 1 summary line |
| ELF spawn debug | 5× println per spawn | 0 |
| Init verbosity | ~20 progress lines | 3 summary lines |
| APIC timer counts | 0 (garbage) | ~73,000,000 (100Hz) |

---


# ZiqaKernel Changelog

## What We've Fixed

| Issue | Fix |
| :--- | :--- |
| **Shell handoff starvation** | Keeps the foreground shell non-preemptible while reading/executing commands and removed the idle input `yield_now()`, preventing Doom/kthreads from stealing serial input. |
| **Shell syscall tools** | Added `syscalls [filter]` and `syscall <name|nr>` so the shell can inspect supported syscall numbers and safely probe harmless kernel syscalls. |
| **VGA color mapping** | Corrected syntax error in VGA color palette initialization for proper CP437-safe rendering. |
| **Linear framebuffer (LFB)** | Enabled and initialized LFB for high-res graphics; fixed hardware blitting path. |
| **nwm-test compilation** | Resolved linker errors and compilation failures; ensured kernel threads have proper stacks. |
| **nwm-test invisible in GUI** | Mirrored the NWM text-mode desktop into the active linear framebuffer and replaced its PID 0 sleep with a foreground-safe frame delay. |
| **nwm-test compositor hang** | Fixed deadlock by spawning compositor and client as separate tasks instead of blocking on the same thread. |
| **Missing SHM/IPC syscalls** | Implemented missing native SHM and IPC syscalls; synchronized Zig client ABI with kernel. |
| **Compositor heap exhaustion** | Resolved heap exhaustion panic by pre-registering the demo surface. |
| **Kernel heap size** | Increased kernel heap to 32MiB to support compositor backbuffers and prevent OOM. |
| **Recursive syscall dispatch** | Removed recursive `dispatch_syscall` from `LinuxAbiPlugin` to prevent stack overflow. |
| **SMP/APIC/Memory compilation errors** | Fixed kernel compilation errors across SMP, APIC, and Memory subsystems during eBPF integration. |
| **Scheduler hardening** | Added `without_interrupts` blocks and interrupt-safe scheduling to prevent race conditions. |
| **Boot sequence logging** | Added extensive serial logging to `init.rs` and `main.rs` for debugging boot failures. |
| **VirtIO PCI register offsets** | Corrected VirtIO network device PCI register offsets for proper device detection. |
| **Double fault on context switch** | Added kernel stack allocation in `spawn_elf()` for all processes (including WASM); static frame allocator array replaced early heap allocation. |
| **VFS not initialized panic** | Moved VFS initialization before self-tests in `init_subsystems()` to prevent uninitialized access during snapshot tests. |
| **WASM loop control flow** | Replaced malformed WASM binary with valid loop module; inline interpreter tests avoid scheduler deadlock. |
| **Capability I/O test failure** | Fixed test to use `pipe:` scheme (which exists and supports read/write) instead of non-existent file. |
| **HPET hang on boot** | Fixed APIC timer calibration hang by ensuring HPET counter is enabled and TSC calibrated after ACPI init. |
| **NVMe AbiError lifetime** | Replaced `alloc::format!` / `AbiError::Other(&msg)` (local String can't satisfy `'static`) with `println!` + static error string in NVMe poll_cq path. |
| **Dynamic linker compilation** | Fixed type inference, unused variable warnings, and memory ordering in `elf_loader.rs` to compile cleanly against nightly kernel ABI. |

## What We've Added

| Feature | Description |
| :--- | :--- |
| **SMP (Multi-Processor)** | AP boot via INIT-SIPI-SIPI protocol, per-CPU data via GS.base MSR, IPI reschedule/TLB shootdown, APIC timer. |
| **ACPI Table Parsing** | RSDP/MADT/FADT parsing for processor topology and interrupt routing (KernelAcpiHandler). |
| **PCI Enumeration** | Full CF8/CFC bus scan (0–255 × 0–31 × 0–7), BAR decoding, class-based device discovery. |
| **Device Driver Model** | Generic `Driver` trait with PCI match/init lifecycle, global `DeviceManager`, block device registry. |
| **PS/2 Mouse Driver** | 3-byte packet decode, signed delta clamping, integrated with NWCC desktop for cursor/window interaction. |
| **AHCI/SATA Driver** | AHCI 1.3 compliant DMA driver with 4K bounce buffers, port multiplication, NCQ-ready architecture. Registers `sata{N}` block devices. |
| **NVMe Driver** | NVMe 1.4 driver with admin/I/O queues, namespace identification, LBA read/write. Registers `nvme{N}n{M}` block devices. |
| **FAT32 (Read-Only)** | Pure Rust FAT32: MBR/BPB parsing, cluster chain walking, short-name directory parsing, recursive VFS mount. |
| **Socket Stack (TCP/UDP)** | Socket state machine via smoltcp, connect/send/recv/close, AF_UNIX loopback, UDP datagrams. |
| **eBPF Hash Maps** | Linear-probing hash map type with insert/update/delete, shared via `Arc<BpfMap>`. |
| **eBPF Tail Calls** | `bpf_tail_call` with ProgArray map type for chaining eBPF programs. |
| **eBPF Tracepoints** | Attach/detach/run for SyscallEntry/SyscallExit tracepoints with verifier gating. |
| **eBPF Bounded Loops** | Instruction-limit-based loop detection instead of strict backward-jump ban. |
| **eBPF Helpers** | `bpf_get_current_comm`, `bpf_probe_read` for safe kernel memory access. |
| **COW Fork** | Full copy-on-write fork with per-process page table cloning, `handle_cow_fault()`. |
| **VMA System** | `Vma` struct with `find_free_range()`, replacing static region model for mmap/brk. |
| **NWCC Desktop Demo** | 80×25 VGA text-mode window manager with 6 apps, mouse/keyboard interaction, double-buffered rendering, animated starfield, taskbar, start menu. |
| **Userspace DRM Driver** | Userspace DRM driver with IPC ioctl dispatch, MMIO mapping via `syscall_dev_map`, and test pattern rendering. |
| **Userspace Net Driver** | VirtIO-Net driver with RX ring management, packet processing, DMA buffer management via `ZIQA_DEV_VIRT_TO_PHYS`, and IRQ event loop. |
| **sys_stat / sys_pipe** | Finalized POSIX syscall additions for filesystem stat and inter-process pipe communication. |
| **sys_waitpid status** | Corrected child exit status reporting in `sys_waitpid`. |
| **POSIX ABI Cleanup** | Synchronized waitpid options, fixed compiler issues, consolidated POSIX implementation. |
| **Debug Instrumentation** | Serial logging across boot sequence, missing exception handlers, `without_interrupts` scheduler hardening. |
| **Redox Port: dtb Scheme** | Device Tree Blob access (`/scheme/dtb`) for ARM/RISC-V bootloader-passed FDT. |
| **Redox Port: memory Scheme** | Direct physical memory access (`/scheme/memory:physical`) with offset tracking via VFS. |
| **Redox Port: user Scheme** | Userspace scheme hosting for running drivers/filesystems in Ring 3 (`/scheme/user`). |
| **Redox Port: debug Scheme** | Enhanced serial/debug console with `disable-vga` special fd and wait-queue input buffer. |
| **Redox Port: sys Scheme** | System info endpoints: `sys:context` (process list), `sys:scheme` (registered schemes), `sys:cpu`, `sys:uptime`. |
| **Redox Port: event Scheme** | Event notification (`/scheme/event`) with `fevent` readiness and `sys_epoll_create1` integration. |
| **Schemes Registry** | Unified `SCHEME_REGISTRY` with `iter_names()` for dynamic `sys:scheme` listing. |
| **VFS Offset Support** | `Scheme` trait extended with `offset` parameter; VFS now manages file pointers. |
| **Capability I/O Test** | `[Path 2] userspace: capability-based I/O` test using `pipe:` scheme for libposix simulation. |
| **WASM Inline Tests** | Self-tests use inline `parse_wasm`/`execute_function` avoiding scheduler deadlocks. |
| **VFS Early Init** | VFS initialized before self-tests to support snapshot tests and capability I/O. |
| **Documentation refresh** | Updated README and compositor IPC docs to describe foreground-safe shell input, syscall inspection/probe commands, and the NWM linear-framebuffer mirror path. |
| **AHCI SATA Driver** | Full AHCI 1.3 driver: PCI BAR5 MMIO mapping, HBA reset/init, port scanning, DMA bounce buffers for PRD page-boundary safety, polling command completion with error decode, `BlockDevice` trait per-port registration. |
| **In-Kernel Dynamic Linker** | `load_elf()` resolves DT_NEEDED shared libraries with DT_GNU_HASH bloom-filter O(1) symbol lookup. Handles R_X86_64_RELATIVE, GLOB_DAT, JUMP_SLOT, COPY, PC32, GOT64 relocations. Library search: DT_RUNPATH → `/lib/` → `/usr/lib/`. 16-byte stack alignment per Linux ABI. Circular dependency detection for `dlopen` cycles. |
| **NVMe Driver (Gap #3)** | Full NVMe controller: PCI BAR3 MMIO, admin queue with identify/namespace commands. Multi-queue I/O (up to 8 queue pairs, round-robin distribution), namespace enumeration (each active NSID → `nvme0n{N}`), DMA bounce buffers for PRP page-boundary safety, controller capability-aware queue sizing, identity logging (vendor/model/serial/fw/LBA format), async event configuration via Set Features. 632 lines. |
| **TTY/PTY Completion** | Blocking reads via `WaitCondition` in PTY scheme, SIGQUIT/SIGTSTP signal dispatch through `send_signal_to_process_group()`, `TIOCSCTTY` fix for session leader attachment, `RingBuf` made public, `SIGTSTP = 20` constant. |
| **Audio Subsystem (Gap #4)** | Intel HDA driver: PCI class 0x04/0x03 detection (Intel/AMD/NVIDIA), MMIO BAR mapping via BAR 0, DMA buffers for CORB/RIRB command interface, immediate verb polling for codec enumeration, `Driver` trait integration. PCI `class_name` extended with "Multimedia" (0x04). 335 lines. |
| **VirtIO GPU / Compositor Support** | VirtIO GPU driver: framebuffer initialization (`init_display`), GPU IPC listener thread, `get_fb_info()` for framebuffer metadata (virtual address, width, height, BPP). New syscall `ZIQA_DEV_GET_GPU_CHAN (1040)` returns GPU IPC channel ID. Two new IPC channels registered at init: Channel 3 (compositor protocol) and Channel 4 (compositor input events). |
| **Kernel-Mode Compositor Protocol** | Display server IPC protocol with opcodes: `CreateSurface`, `Flush`, `Damage` (dirty rect tracking via `DrmRect`), `BufferAttach` (SHM buffer attachment), `SetPosition`, `Input`. Compositor kernel thread (`compositor_main`) and demo client (`demo_client_main`) spawned via `games` feature flag. |
| **Input Subsystem for Compositor** | Keyboard driver exposes `COMPOSITOR_LAST_KEY` (AtomicU16) for ISR-safe key event delivery to compositor. PS/2 mouse driver adds `apply_usb_report()` for xHCI HID mouse events unified with PS/2 input. DRM driver adds `MODE_DAMAGE` ioctl and `DrmRect` for damage tracking. |
| **Audio Subsystem (Gap #4)** | Intel HDA driver: PCI class 0x04/0x03 detection (Intel/AMD/NVIDIA), MMIO BAR mapping via BAR 0, DMA buffers for CORB/RIRB command interface, immediate verb polling for codec enumeration, `Driver` trait integration. PCI `class_name` extended with "Multimedia" (0x04). 335 lines. |
| **nwm-test Enhancements** | Added full desktop mouse interactions (double-click to maximize/restore windows and launch icons, drag-to-select on desktop, taskbar window focus/restore clicks, start button click). Upgraded keyboard controls (WASD support for Snake, `f` key for maximize/restore toggle). Dynamic contextual mouse cursor shapes (resize, drag, selection). Integrated ZRAM memory compression stats (page counts, ratio) into `SysMon` and Terminal `zram`/`neofetch` commands, fixed missing desktop icon for "3D Cube", enabled down arrow menu navigation, and added serial terminal escape sequence mapping to prevent immediate WM exit on arrow key presses. |

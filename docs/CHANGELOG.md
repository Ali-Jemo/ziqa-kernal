# ZiqaKernel Changelog

## What We've Fixed

| Issue | Fix |
| :--- | :--- |
| **VGA color mapping** | Corrected syntax error in VGA color palette initialization for proper CP437-safe rendering. |
| **Linear framebuffer (LFB)** | Enabled and initialized LFB for high-res graphics; fixed hardware blitting path. |
| **nwm-test compilation** | Resolved linker errors and compilation failures; ensured kernel threads have proper stacks. |
| **nwm-test compositor hang** | Fixed deadlock by spawning compositor and client as separate tasks instead of blocking on the same thread. |
| **Missing SHM/IPC syscalls** | Implemented missing native SHM and IPC syscalls; synchronized Zig client ABI with kernel. |
| **Compositor heap exhaustion** | Resolved heap exhaustion panic by pre-registering the demo surface. |
| **Kernel heap size** | Increased kernel heap to 32MiB to support compositor backbuffers and prevent OOM. |
| **Recursive syscall dispatch** | Removed recursive `dispatch_syscall` from `LinuxAbiPlugin` to prevent stack overflow. |
| **SMP/APIC/Memory compilation errors** | Fixed kernel compilation errors across SMP, APIC, and Memory subsystems during eBPF integration. |
| **Scheduler hardening** | Added `without_interrupts` blocks and interrupt-safe scheduling to prevent race conditions. |
| **Boot sequence logging** | Added extensive serial logging to `init.rs` and `main.rs` for debugging boot failures. |
| **VirtIO PCI register offsets** | Corrected VirtIO network device PCI register offsets for proper device detection. |
| **Exit handling in syscall dispatch** | After `sys_exit`, drops process lock and calls `SCHEDULER.schedule()` to prevent return trampoline resuming a dead process. |

## What We've Added

| Feature | Description |
| :--- | :--- |
| **SMP (Multi-Processor)** | AP boot via INIT-SIPI-SIPI protocol, per-CPU data via GS.base MSR, IPI reschedule/TLB shootdown, APIC timer. |
| **ACPI Table Parsing** | RSDP/MADT/FADT parsing for processor topology and interrupt routing (KernelAcpiHandler). |
| **PCI Enumeration** | Full CF8/CFC bus scan (0–255 × 0–31 × 0–7), BAR decoding, class-based device discovery. |
| **Device Driver Model** | Generic `Driver` trait with PCI match/init lifecycle, global `DeviceManager`, block device registry. |
| **PS/2 Mouse Driver** | 3-byte packet decode, signed delta clamping, integrated with NWCC desktop for cursor/window interaction. |
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

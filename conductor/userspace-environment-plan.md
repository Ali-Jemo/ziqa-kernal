# Implementation Plan: Userspace Environment (Bash, Busybox)

## Objective
Enable a functional, interactive userspace environment by building a POSIX-compatibility layer on top of the native Axiq-IQ Capability-based ABI.

## 1. Architectural Strategy: The POSIX-Compatibility Layer
To avoid polluting the native kernel ABI with legacy POSIX requirements, we will implement a userspace library (`libposix`) that maps standard POSIX syscalls (read, write, open, execve, waitpid) to native Axiq-IQ Capability-based system calls.

* **Native Kernel:** Stays pure, capability-focused, and minimal.
* **Compatibility Layer:** All POSIX emulation happens in a userspace shared library/interpreter layer (`libposix.so`).
* **Binary Mapping:** Bash and Busybox will be statically linked (or dynamically linked if we support it) against `libposix`.

## 2. Phase 1: Porting Busybox
Busybox is the essential foundation for a shell environment (ls, cat, cd, etc.).
* **Target:** Port the `busybox` C codebase (statically compiled with `musl` or `zig cc`) against the `libposix` emulation layer.
* **Requirements:**
    * Implement basic `vfs` capability tokens for directory traversal.
    * Map POSIX `open()` to a combination of Capability request + VFS handle.
    * Map POSIX `read()`/`write()` to VFS handle operations.

## 3. Phase 2: Porting Bash
Bash is significantly more complex than Busybox, requiring advanced process and signal management.
* **Target:** Port `bash` against the updated `libposix` emulation layer.
* **Requirements:**
    * **Process Fork/Exec:** Implement `fork` and `exec` by creating new tasks with cloned Capability sets from the parent task.
    * **Signals:** Map POSIX signals to our native IPC signal mechanisms.
    * **Job Control:** Implement TTY handling (via Capability access to Keyboard/Console).

## 4. Integration with Native ABI
We will not change the kernel's fundamental design. If Bash requires a feature not available via the native ABI, we must extend the native ABI with a new `Capability` type, not a specific "POSIX syscall".

## Verification & Testing
* **Busybox:** Run `busybox ls /` and `busybox cat /kernel_log.txt` within the shell.
* **Bash:** Start a shell session, run basic commands, and verify process isolation.
* **Isolation:** Ensure Bash processes cannot access the framebuffer without the appropriate framebuffer Capability token.

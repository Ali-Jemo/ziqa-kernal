# ZiqaKernel Syscall ABI Contract

This document defines the formal ABI contract for ZiqaKernel syscalls. All implementations must adhere to this contract.

## 1. Register Contract (Calling Convention)
ZiqaKernel uses a subset of the x86_64 System V ABI for syscalls via `int 0x80`.

| Register | Usage |
| :--- | :--- |
| `RAX` | Syscall Number (Entry), Return Value (Exit) |
| `RDI` | Argument 0 |
| `RSI` | Argument 1 |
| `RDX` | Argument 2 |
| `R10` | Argument 3 |
| `R8`  | Argument 4 |
| `R9`  | Argument 5 |

## 2. Return & Error Convention
*   **Success**: Syscall handlers must return a non-negative value (>= 0) in `RAX`.
*   **Failure**: Syscall handlers must return a negative value (`-errno`) in `RAX`.
    *   Example: `EPERM` (1) becomes `-1` in `RAX`.
*   **Constraint**: The `syscall_handler` implementation **must not** overwrite `RAX` with any value *other than* the intended return value before executing `iretq`.

## 4. Supported Syscall Families

**Linux x86_64 ABI (112 syscalls implemented):**
*   Core I/O: `read`, `write`, `open`, `close`, `lseek`, `pread64`, `readv`, `writev`
*   Process control: `getpid`, `gettid`, `getppid`, `exit`, `exit_group`, `clone`, `waitpid`, `kill`, `tgkill`
*   Memory management: `brk`, `mmap`, `mprotect`, `munmap`, `madvise`
*   Filesystem: `stat`, `fstat`, `lstat`, `openat`, `getdents64`, `mkdir`, `rmdir`, `unlink`, `rename`, `creat`, `chmod`
*   Signals: `rt_sigaction`, `rt_sigreturn`, `sigaltstack`
*   IPC: `semget`, `semop`, `semctl`, `msgget`, `msgsnd`, `msgrcv`, `shmget`, `shmat`, `shmdt`, `shm_open`, `shm_unlink`
*   Scheduling: `sched_getparam`, `sched_setparam`, `sched_getscheduler`, `sched_setscheduler`, `yield`
*   Time: `nanosleep`, `gettimeofday`, `time`, `clock_gettime`, `clock_getres`
*   Network: `socket`, `bind`, `listen`, `connect`, `accept`, `sendto`, `recvfrom` (feature-gated)
*   Capabilities: `setpriority`, `getpriority`

**WASI ABI (syscall numbers mapped internally):**
*   File I/O: `fd_read`, `fd_write`
*   Process: `args_get`, `args_sizes_get`, `proc_exit`
*   Misc: `sched_yield`, `random_get`

**Redox ABI (for Orbital GUI):**
*   `fmap` — framebuffer memory mapping (SYS_FMAP = 0x20000384)
*   Delegates common syscalls to Linux ABI handlers

---
*Note: This contract supersedes any previous informal implementation conventions.*

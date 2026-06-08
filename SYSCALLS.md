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

## 3. Implementation Status
*   **Functional**: `read`, `write`, `getpid`, `exit`, `fork`, `waitpid`, `nanosleep`, `mmap`, `munmap`, `mprotect`.
*   **Stubbed/Broken**: `stat`, `arch_prctl`, `prctl`. 
    *   *Note*: All `(stub)` implementations must eventually return `-ENOSYS` until fully implemented.

---
*Note: This contract supersedes any previous informal implementation conventions.*

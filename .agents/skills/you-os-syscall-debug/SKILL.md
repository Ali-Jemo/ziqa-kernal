---
name: you-os-syscall-debug
description: Use when debugging ZiqaKernel syscall issues: EINVAL, EFAULT, wrong return values, crashes after syscall, user pointer validation, blocking syscalls, int80/syscall entry.
license: MIT
metadata:
  project: YOU OS / ZiqaKernel
  subsystem: syscalls
---

# YOU OS Syscall Debug Skill

## When to Use

* EINVAL / EFAULT returned
* Wrong return value from syscall
* Crash after syscall
* User pointer validation failure
* Blocking syscall freezes system
* int80 / syscall entry bug

## Mandatory Checks

1. **Syscall number correct?** Verify against userspace ABI.
2. **Arguments in correct registers?** Check calling convention.
3. **Registers preserved?** Per ABI (callee-saved).
4. **User pointers validated?** Before kernel access.
5. **Error codes consistent?**
6. **Blocking syscall yields?** Check scheduler invocation.
7. **Re-entrant safe?**

## Common Patterns

* **EINVAL**: Wrong channel/id/argument or validation mismatch.
* **EFAULT**: User pointer translation/copy problem.
* **Garbage return**: Register convention mismatch.
* **Crash after return**: Bad return frame / clobbered registers.
* **Freeze in syscall**: Blocking path without scheduler yield.

## Do Not Touch

* ELF loader
* Scheduler (unless blocking path proven)
* GUI
* Drivers

Unless evidence points to syscall.

## Minimal Plan

1. Add serial log at syscall entry with number + args.
2. Validate user pointers in `copy_to_user`/`copy_from_user`.
3. Check register save/restore in entry/exit assembly.
4. Apply smallest fix (validation, register, yield).
5. Verify with `cargo check` + QEMU.
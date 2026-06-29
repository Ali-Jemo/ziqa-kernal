---
name: you-os-memory-debug
description: Use when debugging ZiqaKernel memory issues: page faults, frame allocation, heap corruption, paging, virtual memory, demand paging, CR2/CR3 issues, shared memory aliasing, or framebuffer memory mapping.
license: MIT
metadata:
  project: YOU OS / ZiqaKernel
  subsystem: memory
---

# YOU OS Memory Debug Skill

## When to Use

* Page faults at specific addresses (CR2)
* Userspace faults after spawn
* Random crashes after allocation
* Framebuffer/shared memory corruption
* Heap corruption / frame allocator issues
* CR3 / address space switch problems

## Mandatory Checks

1. **Fault address valid?** Read CR2. Is it virtual or physical?
2. **Page mapped?** Check page tables for the faulting address.
3. **Permissions correct?** Present / writable / user bits.
4. **CR3 correct?** Current process using expected address space.
5. **Alignment?** Mapping and ELF segments page-aligned.
6. **Allocator state?** Frame allocator returned overlapping frames?

## Known ZiqaKernel Patterns

* **PID 2 fault at 0x0**: Often null pointer, missing ELF mapping, or broken stack setup.
* **Fault near valid address**: Partial mapping or permission issue.
* **Crash after allocation**: Frame reuse or allocator corruption.
* **GUI buffer corruption**: Framebuffer mapping or SHM aliasing.
* **Works until process switch**: Address space / CR3 issue.

## Do Not Touch

* Scheduler code
* GUI rendering
* Driver initialization
* Syscall dispatch

Unless evidence directly implicates them.

## Minimal Plan

1. Capture fault address + error code from serial.
2. Inspect page tables for that address.
3. Check mapping creation code (ELF loader, SHM, framebuffer).
4. Verify allocator frame uniqueness.
5. Apply smallest fix (alignment, permission, offset).
6. Verify with `cargo check` + QEMU serial capture.
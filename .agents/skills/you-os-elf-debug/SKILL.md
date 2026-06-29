---
name: you-os-elf-debug
description: Use when debugging ZiqaKernel ELF loading issues: userspace faults, PT_LOAD alignment, entry mapping, BSS zeroing, stack mapping, ET_DYN base, shared memory collisions, demand paging.
license: MIT
metadata:
  project: YOU OS / ZiqaKernel
  subsystem: elf-loader
---

# YOU OS ELF Debug Skill

## When to Use

* Userspace process faults immediately after spawn
* `demo_client` does not start
* Fault at entry point
* Fault at 0x0 after process start
* ELF works for one binary but not another
* SHM collision with ELF ranges

## Mandatory Checks

1. **ELF parsing correct?** Class, endianness, architecture.
2. **PT_LOAD segments parsed?** All loadable segments identified.
3. **Virtual addresses page-aligned?** Before mapping.
4. **File offset adjusted?** After alignment.
5. **BSS zeroed?**
6. **Entry mapped executable?**
7. **Stack mapped with user permissions?**
8. **ET_DYN load base handled consistently?**
9. **Ranges collision?** ELF vs SHM/kernel/reserved.

## Known High-Risk Pattern

Non-page-aligned ELF LOAD segments can cause bad demand paging if delta calculations assume page-aligned VMA start.

## Do Not Touch

* Scheduler
* GUI compositor
* Driver init
* IPC logic

Unless evidence points to ELF.

## Minimal Plan

1. Capture fault address + error code.
2. Read ELF loader code (`src/abi/linux/elf_loader.rs`).
3. Verify PT_LOAD mapping loop handles alignment delta.
4. Check BSS zeroing and stack mapping.
5. Apply smallest fix (alignment, permission, offset).
6. Verify with `cargo check` + QEMU run.
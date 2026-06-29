---
name: zk-elf
description: YOU OS ELF loader and userspace loading diagnostic workflow
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-elf

Use the `you-os-kernel-accelerator` workflow.

Focus on ELF/userspace loading.

Task:
$ARGUMENTS

Check:

1. ELF header validity.
2. PT_LOAD parsing.
3. Page alignment of segment virtual addresses.
4. File offset adjustment after page alignment.
5. BSS zeroing.
6. Entry point mapping.
7. Userspace stack mapping.
8. User permissions.
9. ET_EXEC vs ET_DYN handling.
10. SHM/kernel/reserved range collision.
11. Demand paging delta calculations.

Known YOU OS risk:
Non-page-aligned ELF LOAD segments can cause bad demand paging and faults near 0x0.

Do not touch scheduler, GUI, framebuffer, or IPC unless evidence proves ELF is not the cause.

Before patching, output evidence and smallest patch.
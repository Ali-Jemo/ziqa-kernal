---
description: YOU OS ELF loader and userspace loading diagnostic workflow
---

Use skill `you-os-kernel-accelerator`.

Focus on ELF/userspace loading.

Task:
$ARGUMENTS

Check:

1. ELF header validity.
2. PT_LOAD parsing.
3. Page alignment of segment virtual addresses.
4. File offset adjustment after page alignment.
5. BSS zeroing.
5. Entry point mapping.
6. Userspace stack mapping.
7. User permissions.
8. ET_EXEC vs ET_DYN handling.
9. SHM/kernel/reserved range collision.
10. Demand paging delta calculations.

Known YOU OS risk:
Non-page-aligned ELF LOAD segments can cause bad demand paging and faults near 0x0.

Do not touch scheduler, GUI, framebuffer, or IPC unless evidence proves ELF is not the cause.

Before patching, output evidence and smallest patch.
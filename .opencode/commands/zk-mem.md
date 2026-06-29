---
description: YOU OS memory, paging, page faults, frame allocator, heap, SHM workflow
---

Use skill `you-os-kernel-accelerator`.

Focus on memory/paging.

Task:
$ARGUMENTS

Check:

1. Faulting address (CR2) valid?
2. Virtual or physical?
3. Page mapped?
4. Present/writable/user bits?
5. CR3/page table root correct?
6. Current address space correct?
7. Kernel vs user space?
8. Alignment?
9. ELF segment alignment?
10. Allocator frame reuse?
11. Demand paging handler correct?

Do not touch scheduler before memory is ruled out.

Before patching, output evidence and smallest patch.
---
description: YOU OS kernel patch review workflow
---

Use skill `you-os-kernel-accelerator`.

Review the current diff or provided patch for YOU OS / ZiqaKernel.

Task:
$ARGUMENTS

Review for:

1. Kernel/user memory boundary violations.
2. Unsafe Rust without clear invariant.
3. Paging alignment bugs.
4. Syscall ABI breakage.
5. Scheduler blocking/wakeup bugs.
6. Interrupt safety.
7. Locking/deadlock risk.
8. Heap allocation in unsafe contexts.
9. GUI/framebuffer pitch/stride/damage issues.
10. Unrelated rewrites or formatting.
11. Missing verification.
12. Missing error-memory note.

Output:

```text
Patch review:
- Safe to merge:
- Risk level:
- Invariants affected:
- Unsafe blocks:
- Memory risk:
- Scheduler risk:
- ABI risk:
- GUI/runtime risk:
- Missing verification:
- Recommended minimal changes:
```
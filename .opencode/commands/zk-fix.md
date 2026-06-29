---
description: YOU OS full Ponytail kernel bugfix workflow
---

Use skill `you-os-kernel-accelerator`.

You are debugging YOU OS / ZiqaKernel.

User task / arguments:
$ARGUMENTS

Mandatory workflow:

1. Classify the failure stage:
   - compile
   - boot
   - paging/memory
   - interrupt
   - scheduler
   - syscall/ABI
   - ELF/userspace
   - IPC/SHM
   - GUI/framebuffer
   - input/driver
   - panic/hang/triple fault

2. Search project error memory if available:
   - use errmem script or MCP tool
   - compare similar incidents
   - do not blindly reuse old fixes

3. Use Graphify/GitNexus if relationships are unclear:
   - call graph
   - impact analysis
   - subsystem path
   - dependency risk

4. Read exact relevant files before patching.

5. Before patching, output:
   - Subsystem:
   - Failure stage:
   - Evidence:
   - Hypothesis:
   - Files to inspect:
   - Files not to touch:
   - Smallest patch:
   - Verification command:
   - Risk:

6. Apply Ponytail policy:
   - smallest safe diff
   - no rewrites
   - no unrelated formatting
   - no speculative architecture changes

7. Verify using the smallest relevant command.

8. Final report:
   - root cause
   - files changed
   - why this fix
   - verification
   - remaining risk
   - memory note to record
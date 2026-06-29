---
description: YOU OS / ZiqaKernel master kernel workflow
---

Use skill `you-os-kernel-accelerator`.

Handle this YOU OS / ZiqaKernel task using strict kernel discipline.

Task:
$ARGUMENTS

Rules:

1. Determine whether this is:
   - planning
   - debugging
   - patching
   - reviewing
   - architecture
   - performance
   - documentation

2. Choose the correct workflow:
   - bugfix → zk-fix behavior
   - planning → zk-plan behavior
   - code review → zk-review behavior
   - ELF issue → zk-elf behavior
   - scheduler issue → zk-sched behavior
   - GUI/framebuffer issue → zk-gui behavior
   - memory/page fault → zk-mem behavior
   - IPC/SHM issue → zk-ipc behavior
   - input issue → zk-input behavior
   - driver/device issue → zk-driver behavior

3. Use Ponytail minimal-change policy.

4. Do not patch before reading relevant files.

5. Use error memory, MCP, Graphify, or GitNexus when useful.

6. Produce a structured final report.
---
description: YOU OS scheduler, sleep, yield, PID switching, and GUI responsiveness workflow
---

Use skill `you-os-kernel-accelerator`.

Focus on scheduler/process behavior.

Task:
$ARGUMENTS

Check:

1. Which PID is running.
2. Whether timer ticks continue.
3. Whether scheduler is invoked.
4. Whether PID 1 is runnable.
5. Whether PID 2 is runnable/sleeping/blocked.
6. Whether blocking syscalls yield.
7. Whether wakeups occur.
8. Whether context switch saves/restores registers.
9. Whether GUI freeze is only visual or scheduler-related.

Do not touch GUI/framebuffer/mouse files before scheduler evidence is resolved.

Before patching, output evidence and smallest patch.
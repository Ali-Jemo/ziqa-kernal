---
name: zk-gui
description: YOU OS GUI, compositor, framebuffer, dirty region, VNC, and visual freeze workflow
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-gui

Use the `you-os-kernel-accelerator` workflow.

Focus on GUI/framebuffer/compositor.

Task:
$ARGUMENTS

Check:

1. Is serial still alive?
2. Are timer ticks continuing?
3. Is PID 1 scheduled?
4. Is compositor dirty flag set?
5. Is present_rect or full present called?
6. Are damage regions clipped?
7. Is cursor drawn after present?
8. Is framebuffer width/height/pitch/bpp correct?
9. Is the client surface mapped and flushed?
10. Is the issue visual only?

Do not call it a kernel freeze without serial/timer/PID evidence.

Before patching, output evidence and smallest patch.
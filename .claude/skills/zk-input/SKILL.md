---
name: zk-input
description: YOU OS keyboard, mouse, PS/2, USB HID, coordinate transform, click routing, and input redraw workflow.
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-input

Use `you-os-kernel-accelerator` and `you-os-input-debug`.

Focus on input path.

Task:
$ARGUMENTS

Check:
1. Device detected?
2. IRQs firing?
3. EOI sent?
4. Packet decoding correct?
5. Coordinates clamped to active display?
6. Compositor receives event?
7. Event forwarded to correct surface?
8. Redraw triggered after movement/click?
9. Cursor draw order correct?

Do not change resolution hardcodes unless proven stale.

Before patching, output evidence and smallest patch.

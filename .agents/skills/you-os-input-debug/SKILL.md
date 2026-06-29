---
name: you-os-input-debug
description: Use when debugging ZiqaKernel keyboard, mouse, PS/2, USB HID, IRQ delivery, coordinate transforms, click routing, compositor input forwarding, cursor redraw, and input-triggered GUI refresh bugs.
license: MIT
metadata:
  project: YOU OS / ZiqaKernel
  subsystem: input
---

# YOU OS Input Debug

## When to Use

- mouse does not move
- wrong mouse coordinates
- keyboard works but mouse does not
- input IRQs missing
- clicks not delivered
- cursor trails or redraw issues
- input reaches driver but not compositor

## Checks

1. Device detected?
2. IRQs firing?
3. EOI sent?
4. Packet decoding correct?
5. Coordinates clamped to active display bounds?
6. Compositor receives event?
7. Event forwarded to correct surface/window?
8. Redraw triggered after movement/button?
9. Cursor drawn after framebuffer present?

## Do Not Touch

- global resolution constants unless proven stale
- scheduler unless input event delivery blocks
- framebuffer render path unless cursor/present evidence points there

## Minimal Plan

1. Confirm serial input logs.
2. Confirm IRQ path.
3. Confirm coordinate transform.
4. Confirm compositor delivery.
5. Patch only the first broken boundary.

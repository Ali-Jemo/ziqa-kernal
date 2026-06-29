---
name: you-os-gui-framebuffer-debug
description: Use when debugging ZiqaKernel GUI/framebuffer issues: visual freeze, flicker, dirty region, partial present, resolution, cursor trails, window visibility, surface flush, compositor present, pitch/stride/bpp.
license: MIT
metadata:
  project: YOU OS / ZiqaKernel
  subsystem: gui-framebuffer
---

# YOU OS GUI / Framebuffer Debug Skill

## When to Use

* Visual freeze / flicker / drag lag
* Dirty region / partial present bug
* Wrong resolution / cursor trails
* Window not visible / surface not flushed
* Framebuffer pitch/stride/bpp corruption

## Mandatory Checks

1. **Serial/timer alive?** Rule out kernel freeze first.
2. **GUI process scheduled?** Check PID switch logs.
3. **Compositor dirty flag?** Is damage tracked?
4. **present_rect called?** Region correct?
5. **Framebuffer pitch correct?** Width * BPP / 8.
6. **Width/height dynamic?** Handle mode changes.
7. **Damage region clipped?** To screen bounds.
8. **Cursor drawn after present?** Z-order.
9. **Surface ID mapped?** Compositor has correct ID.

## Known Pattern

Visual freeze with serial/timer/PID still active = GUI/compositor issue, NOT kernel freeze.

## Do Not Touch

* Scheduler (unless scheduler proven cause)
* ELF loader
* IPC (unless Flush path proven)
* Drivers

Unless evidence points to GUI.

## Minimal Plan

1. Add serial log on compositor present/dirty.
2. Log framebuffer pitch, width, height, BPP.
3. Verify present_rect region.
4. Check cursor redraw ordering.
5. Apply smallest fix (pitch calc, damage clip, present order).
6. Verify with VNC + serial.
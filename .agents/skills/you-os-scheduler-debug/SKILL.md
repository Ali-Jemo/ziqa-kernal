---
name: you-os-scheduler-debug
description: Use when debugging ZiqaKernel scheduler issues: PID switching, sleep/yield syscalls, timer ticks, GUI freeze during PID 2, blocked syscalls, wakeups, process responsiveness, context switching.
license: MIT
metadata:
  project: YOU OS / ZiqaKernel
  subsystem: scheduler
---

# YOU OS Scheduler Debug Skill

## When to Use

* Screen freezes when switching to PID 2
* GUI resumes when returning to PID 1
* Sleep syscall blocks incorrectly
* Timer ticks continue but GUI does not update
* Process starvation or wakeup issue
* Context switch register save/restore issues

## Mandatory Checks

1. **Timer ticks continue?** Check serial for timer interrupt logs.
2. **Current PID transitions?** Log PID on each switch.
3. **PID 1 runnable?** Is desktop GUI loop ready?
4. **PID 2 state?** Sleeping, runnable, or blocking?
5. **Sleep/yield path inspected?** Before touching GUI code.
6. **Compositor not blamed** before scheduler state checked.

## Known ZiqaKernel Pattern

PID 2 (demo_client) runs → GUI freezes.
PID 1 (desktop) runs → GUI responsive.
This is a scheduler/blocking issue, NOT a compositor bug.

## Do Not Touch

* Framebuffer resolution
* Mouse clamp
* GUI rendering code
* ELF loader

Unless evidence directly implicates them.

## Minimal Plan

1. Add serial log on PID switch.
2. Log task state (runnable/sleeping/blocked).
3. Inspect `syscall::sleep` and `syscall::yield` in `src/abi/syscall.rs`.
6. Check wakeup delivery.
7. Apply smallest fix to blocking path.
8. Verify with `make run-gui` + serial capture.
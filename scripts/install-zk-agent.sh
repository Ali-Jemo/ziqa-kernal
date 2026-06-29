#!/usr/bin/env bash
set -euo pipefail

# Install YOU OS / ZiqaKernel agent integration
# Creates AGENTS.md, slash commands, OpenCode/Kilo/Claude commands, and skills

echo "Installing YOU OS / ZiqaKernel agent integration..."

# Root AGENTS.md
cat > AGENTS.md <<'EOF'
# YOU OS / ZiqaKernel Agent Rules

This repository is a Rust `no_std` operating system kernel project.

For any kernel, GUI, scheduler, ELF, IPC, syscall, framebuffer, boot, QEMU, serial-log, or userspace task:

1. Use the `you-os-kernel-accelerator` skill when available.
2. Use Ponytail policy: smallest safe change only.
3. Do not patch before reading the exact file.
4. Do not guess file contents.
5. Search error memory before debugging if available.
6. Use Graphify/GitNexus for relationships or impact analysis when unclear.
7. Prefer serial logs over visual guessing.
8. Do not treat GUI freeze as kernel freeze without timer/PID evidence.
9. Do not touch unrelated subsystems.
10. After a successful fix, record the incident.

Default verification style:
- Use the smallest relevant check.
- Prefer diagnostic mode before full boot.
- Report exact files changed.
EOF

echo "Created AGENTS.md"

# .opencode/commands
mkdir -p .opencode/commands
mkdir -p .kilo/commands

cat > .opencode/commands/zk-fix.md <<'EOF'
---
description: YOU OS Ponytail kernel bugfix workflow
agent: build
---

Use skill `you-os-kernel-accelerator`.

You are debugging YOU OS / ZiqaKernel.

Task:
$ARGUMENTS

Mandatory workflow:
1. Classify failure stage.
2. Identify subsystem.
3. Search error memory if available.
4. Use Graphify/GitNexus if relationships are unclear.
5. Read exact relevant files.
6. Output evidence, hypothesis, files to inspect, files not to touch, smallest patch, verification, risk.
7. Apply Ponytail minimal-change policy.
8. Verify with the smallest relevant command.
9. Record incident after successful fix.
EOF

cat > .opencode/commands/zk-plan.md <<'EOF'
---
description: YOU OS kernel safe planning workflow
---

Use skill `you-os-kernel-accelerator`.

Plan a kernel change safely.

Task:
$ARGUMENTS

Rules:
1. Identify affected subsystems.
2. Check error memory for related incidents.
3. Use Graphify/GitNexus for impact analysis.
4. List all files that will be touched.
5. State verification commands.
6. Apply Ponytail minimal-change policy.
7. No architecture rewrites unless explicitly requested.
EOF

cat > .opencode/commands/zk-review.md <<'EOF'
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
EOF

cat > .opencode/commands/zk-elf.md <<'EOF'
---
description: YOU OS ELF loader and userspace loading diagnostic workflow
---

Use skill `you-os-kernel-accelerator`.

Focus on ELF/userspace loading.

Task:
$ARGUMENTS

Check:
1. ELF header validity.
2. PT_LOAD parsing.
3. Page alignment of segment virtual addresses.
4. File offset adjustment after page alignment.
5. BSS zeroing.
5. Entry point mapping.
6. Userspace stack mapping.
7. User permissions.
8. ET_EXEC vs ET_DYN handling.
9. SHM/kernel/reserved range collision.
10. Demand paging delta calculations.

Known YOU OS risk:
Non-page-aligned ELF LOAD segments can cause bad demand paging and faults near 0x0.

Do not touch scheduler, GUI, framebuffer, or IPC unless evidence proves ELF is not the cause.

Before patching, output evidence and smallest patch.
EOF

cat > .opencode/commands/zk-sched.md <<'EOF'
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
EOF

cat > .opencode/commands/zk-gui.md <<'EOF'
---
description: YOU OS GUI, compositor, framebuffer, dirty region, VNC, and visual freeze workflow
---

Use skill `you-os-kernel-accelerator`.

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
EOF

cat > .opencode/commands/zk-mem.md <<'EOF'
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
EOF

cat > .opencode/commands/zk-ipc.md <<'EOF'
---
description: YOU OS IPC, SHM, event channel, Flush, and client protocol workflow
---

Use skill `you-os-kernel-accelerator`.

Focus on IPC/shared memory/client protocol.

Task:
$ARGUMENTS

Check:
1. Does the channel exist?
2. Does sender/receiver own the handle?
3. Are channel IDs confused with SHM IDs?
4. Is receive blocking?
5. Does blocking receive yield?
6. Are wakeups delivered?
7. Are message sizes validated?
8. Is SHM attached at expected virtual address?
9. Is RegisterEventChannel correct?
10. Is Flush handled by compositor?

Before patching, output evidence and smallest patch.
EOF

cat > .opencode/commands/zk-input.md <<'EOF'
---
description: YOU OS keyboard, mouse, PS/2, USB HID, coordinates, click routing, and input redraw workflow
---

Use skill `you-os-kernel-accelerator`.

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
EOF

cat > .opencode/commands/zk-driver.md <<'EOF'
---
description: YOU OS hardware driver, PCI, VirtIO, MMIO, IRQ, and device init workflow
---

Use skill `you-os-kernel-accelerator`.

Focus on drivers/device behavior.

Task:
$ARGUMENTS

Check:
1. Device detected?
2. BAR/MMIO/port address correct?
3. MMIO mapping/cache attributes correct?
4. IRQ routed/unmasked?
5. EOI/ack path correct?
6. Init order follows device spec?
7. DMA buffer alignment/ownership correct?
8. Physical vs virtual address usage correct?
9. Volatile register access documented with SAFETY.

Before patching, output evidence and smallest patch.
EOF

cat > .opencode/commands/zk.md <<'EOF'
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
EOF

# Copy to Kilo
cp .opencode/commands/zk*.md .kilo/commands/

echo "Created OpenCode and Kilo commands"

# .claude/skills for slash commands
mkdir -p .claude/skills/zk
mkdir -p .claude/skills/zk-fix
mkdir -p .claude/skills/zk-plan
mkdir -p .claude/skills/zk-review
mkdir -p .claude/skills/zk-elf
mkdir -p .claude/skills/zk-sched
mkdir -p .claude/skills/zk-gui
mkdir -p .claude/skills/zk-mem
mkdir -p .claude/skills/zk-ipc
mkdir -p .claude/skills/zk-input
mkdir -p .claude/skills/zk-driver

cat > .claude/skills/zk/SKILL.md <<'EOF'
---
name: zk
description: YOU OS / ZiqaKernel master kernel workflow
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk

Use the `you-os-kernel-accelerator` workflow.

Handle this YOU OS / ZiqaKernel task using strict kernel discipline.

Task:
$ARGUMENTS

Rules:
1. Determine whether this is: planning, debugging, patching, reviewing, architecture, performance, documentation
2. Choose the correct workflow: bugfix→zk-fix, planning→zk-plan, review→zk-review, ELF→zk-elf, scheduler→zk-sched, GUI→zk-gui, memory→zk-mem, IPC→zk-ipc, input→zk-input, driver→zk-driver
3. Use Ponytail minimal-change policy.
4. Do not patch before reading relevant files.
5. Use error memory, MCP, Graphify, or GitNexus when useful.
6. Produce a structured final report.
EOF

cat > .claude/skills/zk-fix/SKILL.md <<'EOF'
---
name: zk-fix
description: Run the YOU OS / ZiqaKernel Ponytail kernel bugfix workflow. Use for bugs, panics, freezes, page faults, scheduler issues, ELF loading, IPC, syscalls, GUI/framebuffer, QEMU, serial logs, Rust no_std kernel problems.
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-fix

Use the `you-os-kernel-accelerator` workflow.

Mandatory workflow:
1. Classify failure stage.
2. Search error memory if available.
3. Use Graphify/GitNexus if relationships are unclear.
4. Read exact files before patching.
5. State evidence and smallest patch before editing.
6. Apply Ponytail minimal-change policy.
7. Verify.
8. Record incident.
EOF

cat > .claude/skills/zk-plan/SKILL.md <<'EOF'
---
name: zk-plan
description: YOU OS kernel safe planning workflow
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-plan

Use the `you-os-kernel-accelerator` workflow.

Plan a kernel change safely.

Task:
$ARGUMENTS

Rules:
1. Identify affected subsystems.
2. Check error memory for related incidents.
3. Use Graphify/GitNexus for impact analysis.
4. List all files that will be touched.
5. State verification commands.
6. Apply Ponytail minimal-change policy.
7. No architecture rewrites unless explicitly requested.
EOF

cat > .claude/skills/zk-review/SKILL.md <<'EOF'
---
name: zk-review
description: YOU OS kernel patch review workflow
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-review

Use the `you-os-kernel-accelerator` workflow.

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
11. Missing error-memory note.

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
EOF

cat > .claude/skills/zk-elf/SKILL.md <<'EOF'
---
name: zk-elf
description: YOU OS ELF loader and userspace loading diagnostic workflow
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-elf

Use the `you-os-kernel-accelerator` workflow.

Focus on ELF/userspace loading.

Task:
$ARGUMENTS

Check:
1. ELF header validity.
2. PT_LOAD parsing.
3. Page alignment of segment virtual addresses.
4. File offset adjustment after page alignment.
5. BSS zeroing.
5. Entry point mapping.
6. Userspace stack mapping.
7. User permissions.
8. ET_EXEC vs ET_DYN handling.
9. SHM/kernel/reserved range collision.
10. Demand paging delta calculations.

Known YOU OS risk:
Non-page-aligned ELF LOAD segments can cause bad demand paging and faults near 0x0.

Do not touch scheduler, GUI, framebuffer, or IPC unless evidence proves ELF is not the cause.

Before patching, output evidence and smallest patch.
EOF

cat > .claude/skills/zk-sched/SKILL.md <<'EOF'
---
name: zk-sched
description: YOU OS scheduler, sleep, yield, PID switching, and GUI responsiveness workflow
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-sched

Use the `you-os-kernel-accelerator` workflow.

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
EOF

cat > .claude/skills/zk-gui/SKILL.md <<'EOF'
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
EOF

cat > .claude/skills/zk-mem/SKILL.md <<'EOF'
---
name: zk-mem
description: YOU OS memory, paging, page fault, frame allocator, heap, SHM workflow
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-mem

Use the `you-os-kernel-accelerator` workflow.

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
EOF

cat > .claude/skills/zk-ipc/SKILL.md <<'EOF'
---
name: zk-ipc
description: YOU OS IPC, SHM, event channel, Flush, and client protocol workflow
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-ipc

Use the `you-os-kernel-accelerator` workflow.

Focus on IPC/shared memory/client protocol.

Task:
$ARGUMENTS

Check:
1. Does the channel exist?
2. Does sender/receiver own the handle?
3. Are channel IDs confused with SHM IDs?
4. Is receive blocking?
5. Does blocking receive yield?
6. Are wakeups delivered?
7. Are message sizes validated?
8. Is SHM attached at expected virtual address?
9. Is RegisterEventChannel correct?
10. Is Flush handled by compositor?

Before patching, output evidence and smallest patch.
EOF

cat > .claude/skills/zk-input/SKILL.md <<'EOF'
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
EOF

cat > .claude/skills/zk-driver/SKILL.md <<'EOF'
---
name: zk-driver
description: YOU OS hardware driver, PCI, VirtIO, MMIO, IRQ, DMA, timer, UART, block, GPU, and device init workflow.
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-driver

Use `you-os-kernel-accelerator` and `you-os-driver-debug`.

Focus on drivers/device behavior.

Task:
$ARGUMENTS

Check:
1. Device detected?
2. BAR/MMIO/port address correct?
3. MMIO mapping/cache attributes correct?
4. IRQ routed/unmasked?
5. EOI/ack path correct?
6. Init order follows device spec?
7. DMA buffer alignment/ownership correct?
8. Physical vs virtual address usage correct?
9. Volatile register access documented with SAFETY.

Before patching, output evidence and smallest patch.
EOF

echo "Created Claude skills"
echo ""
echo "Installation complete! Available commands:"
echo "  /zk, /zk-fix, /zk-plan, /zk-review, /zk-elf, /zk-sched, /zk-gui, /zk-mem, /zk-ipc, /zk-input, /zk-driver"
echo ""
echo "For OpenCode: use .opencode/commands/"
echo "For Kilo: use .kilo/commands/"
echo "For Claude: use .claude/skills/zk*"
echo ""
echo "Root rules: AGENTS.md"
echo "Master skill: .agents/skills/you-os-kernel-accelerator/SKILL.md"
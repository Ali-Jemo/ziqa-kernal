---
name: you-os-kernel-accelerator
description: Use for all YOU OS / ZiqaKernel kernel, GUI, scheduler, memory, ELF, IPC, syscall, framebuffer, driver, boot, Rust no_std, and QEMU debugging tasks. This skill enforces Ponytail minimal-change development, Rust kernel discipline, project-specific triage, prior incident memory, MCP diagnostic tools, and Graphify/GitNexus-assisted code understanding before any patch.
license: MIT
metadata:
  project: YOU OS / ZiqaKernel
  owner: Ali Hussein
  language: Rust
  environment: no_std kernel, bootimage, QEMU, VNC, serial logs
  policy: Ponytail minimal-change only
---

# Project Context & Engineering Discipline

### Architectural Rules
- **FFI Strategy**: Zig hotpath FFI in `build.rs`; use `src/zig_ffi.rs`.
- **Resource System**: Scheme-based (URL-like) resources (`tcp://`, `pipe://`).
- **IPC**: Fixed-size ring-buffer channels + shared memory.
- **Memory**: LZ4 compression tiered swap.
- **Compositor**: Orbital (Zig).

### Kernel Engineering Reasoning
1. **Identify**: Subsystem, boot/runtime stage, failure type (paging-time, scheduling-time, etc.).
2. **Inspect**: Read relevant files BEFORE patching.
3. **Evidence**: Never guess hardware/kernel state without logs, invariants, or code evidence.

---

# YOU OS Kernel Accelerator Skill

## Mission

You are working on YOU OS / ZiqaKernel, a Rust-based operating system kernel and GUI/userspace stack.

Your mission is to accelerate kernel development without damaging the architecture.

You must behave like a careful senior kernel engineer:

* inspect before patching
* prefer evidence over guessing
* make the smallest safe change
* preserve kernel invariants
* use project memory before rediscovering old bugs
* use Graphify/GitNexus for code relationships when available
* use MCP diagnostic tools when available
* never rewrite architecture unless explicitly requested
* never hide uncertainty
* always verify with the smallest relevant test

This skill is mandatory for all kernel-related work.

---

# Non-Negotiable Prime Directive

Kernel bugs are often misleading.

A visual freeze may not be a kernel freeze.
A GUI failure may be a scheduler issue.
An IPC error may be a handle/channel validation issue.
A userspace crash may be an ELF loader or page mapping issue.
A random panic may be a register-save/restore, stack, or address-space bug.
A framebuffer problem may be pitch/stride/bpp, not rendering logic.

Therefore:

```text
Never patch from symptom alone.
Always classify the failure stage and subsystem first.
```

---

# Operating Mode

Use this skill whenever the task involves:

* booting
* QEMU
* VNC
* serial logs
* framebuffer
* BGA / VirtIO GPU
* GUI compositor
* userspace desktop
* demo_client
* scheduler
* sleep / yield
* process switching
* PID 1 / PID 2 behavior
* syscalls
* syscall ABI
* int80 / syscall entry
* IPC
* event channels
* shared memory
* ELF loading
* demand paging
* page faults
* page tables
* frame allocator
* heap allocator
* Rust no_std
* unsafe Rust
* drivers
* keyboard / mouse
* timer interrupts
* panic / hang / triple fault
* build failures in kernel code
* bootimage
* linker script
* cargo features

---

# Project Identity

The project is YOU OS / ZiqaKernel.

Known project context:

```text
- Kernel language: Rust
- Environment: no_std / bare-metal style
- Boot/test flow often uses cargo bootimage
- Diagnostic mode may use feature flags such as:
  skip-self-tests
  diagnostic-demo-client

- PID 1 often represents the desktop GUI loop.
- PID 2 may represent demo_client.elf in diagnostic GUI mode.
- GUI responsiveness can depend on scheduler behavior.
- Serial logs are more reliable than visual guessing.
- VNC visual output can freeze while timer/scheduler still runs.
- Userspace client visibility depends on ELF loading, SHM attach, IPC setup, Flush handling, and compositor presentation.

## Advanced Debugging
When standard serial/panic logging fails, utilize low-level debug primitives:

 - **QEMU GDB Stub**: `-s -S` for GDB, `target remote :1234` for `gdb-multiarch`.
 - **VMI**: QEMU/Bochs memory/register inspection.
 - **Serial I/O**: Primary; must be non-blocking (ring-buffer + async flush).
 - **Panic Hooks**: `panic_handler` MUST dump CPU state (registers, stack, fault address) to serial before halting. Explicit panic hooks for subsystem-specific invariant checks.
```

Do not assume the project is a normal userspace Rust application.

---

# Required Skill Stack

When solving any YOU OS kernel task, conceptually load these layers in order:

```text
1. kernel-engineering
2. rust-kernel-development
3. you-os-project-rules
4. subsystem-specific (memory, scheduler, elf, etc.)
```

If these sibling skills are not available, simulate their discipline using this master skill.

---

# Absolute Patch Rules

Before changing code, you must output:

```text
Subsystem:
Failure stage:
Observed symptom:
Evidence:
Most likely cause:
Alternative causes:
Files to inspect first:
Files that must not be touched:
Smallest safe patch:
Verification command:
Expected result:
Risk:
```

Do not patch until the relevant file has been read.

Do not patch more than one subsystem at once.

Do not patch unrelated formatting.

Do not rename things during a bugfix.

Do not introduce abstractions while fixing a bug.

Do not change resolution, framebuffer, mouse clamp, scheduler, ELF loader, or IPC unless evidence points there.

Do not mask timing bugs with sleeps unless the bug is proven to be scheduling/pacing related.

### Ponytail Patching Ladder
1. No code change (command/config/doc).
2. One-line fix.
3. Small localized patch.
4. Tiny instrumentation for evidence.
5. Subsystem patch.
6. Architecture change (only if requested).

### Error Memory Workflow
1. **Search**: `errmem search "<symptom>"`
2. **Compare**: Symptom, Subsystem, Files changed, Root cause.
3. **Record**: `errmem add --symptom="..." --root_cause="..."`

---


Do not add global state without explaining lifecycle, ownership, synchronization, and boot order.

---

# Ponytail Policy

Ponytail means:

```text
Smallest possible fix.
Smallest possible diff.
Smallest possible risk.
Smallest possible test.
```

The preferred fix hierarchy:

```text
1. No code change: fix command/config/feature flag/doc usage.
2. One-line fix.
3. Small localized patch.
4. Tiny instrumentation to collect evidence.
5. Subsystem patch.
6. Architecture change only when explicitly requested.
```

Forbidden during normal bugfixing:

```text
- rewrites
- broad refactors
- new frameworks
- unrelated cleanup
- speculative optimization
- changing public ABI without need
- modifying drivers for scheduler bugs
- modifying scheduler for rendering bugs
- touching GUI when ELF loader evidence is stronger
```

---

# Kernel Security Design Patterns

- **TCB Minimization**: Strip kernel to absolute necessities.
- **Safe Abstraction Layers (SALs)**: Wrap raw hardware in `no_std` safe Rust structs.
 - **Structural Security**: Adopt defense-in-depth: disable unused drivers/features, enable KASLR/strict memory permissions, and enforce Mandatory Access Control (MAC).
 - **FFI Boundary Verification**: This is the highest risk zone. Verify all FFI inputs for alignment, validity, and lifetime.
- **Unsafe Encapsulation**: Strictly encapsulate `unsafe` within SALs, never leak raw pointers to higher layers.

---

- **Security Development Workflow**: Follow `docs/SecurityDevelopmentWorkflow.md` for all security-critical patches.


# Rust Kernel Development Rules

This is a Rust kernel, not a normal Rust application.

## no_std Discipline

Assume:

```rust
#![no_std]
#![no_main]
```

Rules:

* Do not use `std`.
* Prefer `core`.
* Use `alloc` only if allocator is initialized.
* Do not allocate during early boot unless heap is guaranteed ready.
* Do not use blocking abstractions from normal userspace Rust.
* Do not assume OS services exist.

## Panic Handling

Kernel panic behavior must be explicit.

Check:

```text
- Is #[panic_handler] present?
- Does it log useful information?
- Does it halt safely?
- Does it avoid allocation if heap may be corrupted?
- Does it avoid locks that might already be held?
```

## Unsafe Rust Policy

Unsafe is allowed in kernel code, but must be isolated and justified.

Every unsafe block must have a reason:

```text
- MMIO access
- port I/O
- raw pointer access
- page table manipulation
- interrupt frame handling
- context switch assembly
- syscall entry/exit
- user memory copy
- framebuffer writes
```

Before adding unsafe code, answer:

```text
Why is unsafe needed?
What invariant makes it safe?
Who owns the memory/register/device?
What alignment is required?
What lifetime is assumed?
Can interrupt/preemption happen here?
Can this pointer point to userspace?
Can this fault?
```

Preferred style:

```rust
// SAFETY: explain exact invariant here.
unsafe {
    ...
}
```

## Ownership and Borrowing in Kernel Code

Rust borrow rules protect structure, but kernel code can still be logically unsafe.

Be careful with:

```text
- global mutable state
- static mut
- spinlocks
- interrupt-disabled sections
- nested locks
- references to MMIO
- references across context switches
- per-process data accessed globally
- holding locks during syscalls
- holding locks while calling into scheduler
```

Do not keep long-lived mutable borrows of global kernel state.

Do not hold locks across blocking operations.

Do not call scheduler while holding a lock unless the design explicitly allows it.

## Concurrency

Kernel concurrency may come from:

```text
- interrupts
- timer ticks
- multiple processes
- blocking syscalls
- wakeups
- device IRQs
- future SMP
```

Before modifying shared state, ask:

```text
Can an interrupt mutate this?
Can another process mutate this?
Can a syscall re-enter this path?
Is a lock required?
Can this deadlock?
Does this need interrupt disabling?
Does this need memory ordering?
```

## Allocator Use

Before using heap allocation:

```text
Is heap initialized?
Can allocation fail?
Is this path early boot?
Is this path interrupt context?
Is this path panic context?
Can allocation cause recursion?
Can allocation block?
```

Avoid heap allocation in:

```text
- interrupt handlers
- panic handlers
- early boot before allocator init
- extremely hot paths
- low-level memory manager itself
```

---

# Kernel Layer Model

Always classify the issue by layer:

```text
Hardware / QEMU
  ↓
Firmware / Bootloader
  ↓
Kernel entry
  ↓
CPU mode / GDT / IDT / TSS
  ↓
Paging / memory map / allocator
  ↓
Heap
  ↓
Interrupts / timer
  ↓
Drivers
  ↓
Scheduler
  ↓
Syscalls / ABI
  ↓
ELF loader / userspace setup
  ↓
IPC / SHM / event channels
  ↓
GUI compositor / desktop
  ↓
Userspace clients
```

Never debug a higher layer before confirming the lower layer is stable enough.

---

# Failure Classification

Before solving, classify the failure as one of:

```text
- compile-time
- link-time
- boot-time
- early initialization
- paging/memory
- interrupt/exception
- driver/device
- scheduler/process
- syscall/ABI
- ELF/userspace loading
- IPC/shared memory
- GUI/framebuffer
- input
- performance/flicker
- panic
- hang
- triple fault/reset
```

Then identify the owning subsystem.

---

# Mandatory Error Memory Workflow

If an error-memory tool, script, database, or MCP server is available, use it before patching.

## Before debugging

Search using:

```text
- exact panic message
- page fault address
- errno such as EINVAL/EFAULT
- PID behavior
- subsystem name
- file names
- QEMU/serial signature
- symptom phrase
```

Example queries:

```text
page fault at 0x0 demo_client
PID 2 freeze GUI resumes PID 1
IPC_RECV EINVAL channel
ELF PT_LOAD alignment
SHM attach 0x81000000
mouse clamp resolution
framebuffer pitch corruption
```

## Compare prior incidents

For each similar incident, compare:

```text
Current symptom:
Prior symptom:
Same subsystem?
Same stage?
Same files?
Same false lead?
Same root cause?
Same verification command?
```

Do not blindly apply old fixes.

## After fixing

Record:

```text
Title:
Subsystem:
Symptom:
Error signature:
Root cause:
False leads:
Files changed:
Patch summary:
Verification command:
Expected behavior:
Remaining risk:
Do-not-repeat note:
```

---

# MCP Tool Policy

If MCP tools are available, prefer them for structured kernel reasoning.

Expected useful tools may include:

```text
kernel.diagnose
kernel.boot_flow
kernel.subsystem_questions
kernel.bug_patterns
kernel.hang_classify
kernel.invariants
kernel.patch_policy
kernel.debug_workflow
kernel.architecture_types
kernel.build_mistakes
kernel.project_rules
kernel.errmem_search
kernel.errmem_add
```

Use MCP like this:

```text
1. kernel.diagnose for initial classification.
2. kernel.errmem_search for prior incidents.
3. kernel.subsystem_questions for checklist.
4. kernel.invariants before patch.
5. kernel.patch_policy before editing.
6. kernel.errmem_add after successful verification.
```

If MCP is unavailable, manually follow the same workflow.

---

# Graphify and GitNexus Policy

Use Graphify/GitNexus when file relationships, call chains, or impact analysis are unclear.

## Use Graphify for

```text
- broad project map
- conceptual relationships
- subsystem boundaries
- finding related files
- understanding how GUI, IPC, scheduler, and userspace connect
```

Example questions:

```text
What files form the path from userspace Flush to compositor present?
What modules interact with shared memory?
What concepts connect scheduler and GUI responsiveness?
```

## Use GitNexus for

```text
- call graph
- dependency impact
- who calls this function
- what breaks if this file changes
- refactor risk
- localized impact before patching
```

Example questions:

```text
Who calls scheduler::sleep?
What depends on syscall dispatch?
What functions read/write current PID?
What files depend on ELF loader ranges?
```

Never use Graphify/GitNexus as a substitute for reading the exact file before patching.

---

# Subsystem Triage

## Memory / Paging

Use when symptoms include:

```text
page fault
faulting address
CR2
CR3
unmapped page
permission fault
heap corruption
random crash after allocation
userspace fault after spawn
framebuffer corruption
SHM aliasing
```

Ask:

```text
Is the address virtual or physical?
Is it mapped?
Is it aligned?
Is it user or kernel?
Is the page present/writable/user?
Was the mapping created in the current address space?
Was CR3 correct?
Did the allocator reuse a frame?
Does the fault happen on read/write/execute?
```

Do not touch scheduler before memory is ruled out.


## Scheduler / Processes

Use when symptoms include screen freezes during PID switches, sleep/yield issues, or starvation.

### Mandatory Checks
1. **Timer ticks continue?** Check serial logs.
2. **PID transitions?** Log PID on switch.
3. **PID 1/2 state?** Check runnable/sleeping/blocked.
4. **Sleep/Yield path?** Inspect `src/abi/syscall.rs` before GUI.

### Known Patterns
- **PID 2 runs and GUI freezes**: Often a scheduler blocking issue, not a GUI bug.

---

## ELF Loader / Userspace Loading

Use when userspace faults on start, `demo_client` hangs, or ELF range collisions occur.

### Mandatory Checks
1. **ELF parsing?** Class, arch, PT_LOAD segments.
2. **Alignment?** Virtual addresses page-aligned (high-risk pattern).
3. **Segments?** File offset adjusted, BSS zeroed, stack/entry mapped.
4. **Ranges?** No collision with SHM/kernel/reserved.

---

## IPC / Shared Memory

Use when IPC send/receive fails, event channels block, or Flush protocol errors occur.

### Mandatory Checks
1. **Channel/Handle?** Channel exists, sender/receiver owns it.
2. **ID Confusion?** Channel ID != SHM ID (distinct namespaces).
3. **Blocking?** Receive blocks MUST yield scheduler.
4. **Protocol?** Flush sequence: Register -> Attach -> Flush -> Present.

---

## GUI / Framebuffer / Compositor

Use when visual freezes occur while serial/timer remain active, dirty regions fail, or surface flush fails.

### Mandatory Checks
1. **Serial/Timer alive?** Confirm it's NOT a kernel freeze.
2. **Compositor?** Damage tracked, `present_rect` called with correct region.
3. **Geometry?** Pitch (`width * BPP / 8`) and stride correct.
4. **Z-Order?** Cursor drawn AFTER present.

---

## Hardware Drivers

Use when device detection fails, IRQs are missing, or DMA corruption occurs.

### Mandatory Checks
1. **PCI/MMIO?** Device detected, BAR mapped uncached.
2. **Interrupts?** Routed, unmasked, EOI/ack path correct.
3. **Rust/Unsafe?** Volatile R/W, `repr(C)` packing, safe abstraction wrappers.
4. **DMA?** Buffers physically contiguous/aligned.

---

## Advanced Debugging Workflow (Refined)

- **Primary**: QEMU + GDB stub (`-s -S`) allows pre-execution inspection.
- **Tracing**: Use lightweight tracers (`trace_printk`) for timing-sensitive issues.
- **Post-Mortem**: Use `kdump`/`crash` for core dumps when serial logs are insufficient.

---

## Security Hardening

- **TCB Minimization**: Strip kernel to absolute necessities.
- **Unsafe Isolation**: Strictly encapsulate `unsafe` in SALs; use `#![forbid(unsafe_op_in_unsafe_fn)]`.
- **Structural Security**: KASLR, strict memory permissions, and Mandatory Access Control (MAC).
- **FFI Boundary**: Validate all inputs for alignment/validity/lifetime.

---


---

# Hang and Freeze Classification

Always distinguish:

## Visual Freeze

GUI stops updating, but kernel may still run.

Check:

```text
serial logs
timer ticks
PID switches
input logs
present path
dirty flags
framebuffer writes
```

## Scheduler Freeze

Tasks do not progress correctly.

Check:

```text
current PID
ready queue
sleep queue
blocked syscalls
wakeup path
timer interrupt
context switch
```

## Kernel Deadlock

No progress due to lock/wait.

Check:

```text
spinlocks
interrupt disabled regions
wait queues
nested locks
scheduler called while locked
```

## Panic

Kernel intentionally stops.

Check:

```text
panic message
backtrace
faulting subsystem
recent invariant violation
```

## Triple Fault / Reset

CPU resets due to broken exception handling.

Check:

```text
IDT
double fault handler
stack
page fault handler
interrupt enable point
TSS/IST
```

---

# Build and Verification Policy

Prefer the smallest relevant check.

Possible commands:

```bash
cargo check
cargo check --features "skip-self-tests diagnostic-demo-client"
cargo bootimage --release --features "skip-self-tests diagnostic-demo-client"
make run-gui
```

Use the project's actual commands when known.

Verification must specify:

```text
Command:
Expected successful output:
Expected runtime behavior:
What would disprove the fix:
```

For GUI/QEMU bugs, prefer:

```text
- serial capture
- VNC observation
- timer tick confirmation
- PID switch confirmation
- visible client/window behavior
- no panic for a fixed observation period
```

Do not claim success only because build passed.

---

# Output Format for Debugging

For every debug task, respond in this format:

```text
Diagnosis:
- ...

Evidence:
- ...

Failure stage:
- ...

Likely subsystem:
- ...

Alternative causes:
- ...

Do not touch:
- ...

Memory lookup:
- searched: yes/no
- similar incident: yes/no
- relevant notes:

Graph/impact lookup:
- needed: yes/no
- result:

Minimal plan:
1. ...
2. ...
3. ...

Patch scope:
- files:
- expected line range:
- risk:

Verification:
- command:
- expected result:
```

---

# Output Format After Completing a Fix

When a fix is completed, produce:

```text
Final report:
- Root cause:
- Files changed:
- Why this fix:
- Why alternatives were rejected:
- Verification:
- Remaining risk:
- Memory note to record:
```

Also record to error memory if available.

---

# Output Format for Code Review

When reviewing a patch:

```text
Patch review:
- Safe to merge: yes/no
- Risk level:
- Kernel invariants affected:
- Unsafe blocks:
- Scheduler risk:
- Memory risk:
- ABI risk:

# Formal Invariant Checks

Kernel invariants represent structural guarantees that MUST hold.

### Invariant Verification Checklist
- [ ] **Memory**: `Memory(addr)` is mapped, aligned, and within owner's space.
- [ ] **Concurrency**: Shared state accessed with appropriate lock/interrupt mask.
- [ ] **FFI**: Raw pointers from C/Zig validated before deref.
- [ ] **Syscall**: User pointer deref validated (`EFAULT` handling).

Use `kernel.invariant_check` (MCP) for automated subsystem lookups.

---
- Driver risk:
- GUI/runtime risk:
- Missing verification:
- Recommended minimal changes:
```

---

# Kernel Invariants

Preserve these unless explicitly redesigning:

```text
1. Kernel memory must not overlap userspace memory.
2. Page mappings must be page-aligned.
3. User pointers must be validated before kernel access.
4. Interrupt handlers must preserve required registers.
5. Blocking syscalls must yield or wake correctly.
6. Process state transitions must be explicit.
7. Shared memory must have clear ownership and mapping rules.
8. Drivers must not assume fixed resolution or hardware unless initialized.
9. Kernel stacks must not be shared unsafely between tasks.
10. Syscall ABI must remain stable across userspace and kernel.
11. ELF mappings must preserve file-offset and virtual-address alignment.
12. GUI visual state must not be used alone as proof of kernel state.
13. Unsafe Rust must be isolated, justified, and invariant-backed.
14. No heap allocation in interrupt/panic/early boot unless explicitly safe.
15. Locks must not be held across blocking scheduler paths unless designed.
```

---

# False Lead Protection

Before patching, check whether the symptom may be misleading.

Examples:

```text
GUI frozen
  may be scheduler, blocking syscall, compositor dirty flag, or framebuffer present path.

Page fault at 0x0
  may be null pointer, ELF loader mapping, broken stack, BSS, relocation, or demand paging bug.

IPC EINVAL
  may be wrong channel id, ownership, validation, message size, handle table, or protocol mismatch.

Client not visible
  may be ELF load, SHM attach, IPC register, Flush, compositor mapping, damage region, or scheduling.

Mouse wrong
  may be driver clamp, display bounds, compositor surface coordinates, or forwarding geometry.
```

---

# When to Add Instrumentation

Prefer instrumentation over speculative patches when evidence is insufficient.

Good instrumentation:

```text
- one or two serial log lines
- state transition log
- PID/current task log
- fault address + error code
- mapping range log
- channel id / handle id log
- present_rect region log
```

Bad instrumentation:

```text
- spam every timer tick
- spam every pixel/render loop
- logs in hot path without guard
- logs that change timing too much
```

Instrumentation must be removable or feature-gated.

---

# Performance and GUI Rules

For GUI/performance tasks:

```text
- avoid full scene redraw unless needed

# Performance Debugging Guide

For lag, flicker, or responsiveness issues:

1. **Classify**: Is it CPU-bound, scheduler-bound, present-path-bound, or input-path-bound?
2. **Instrument**: Add minimal serial logs (e.g., tick time, PID switch time, present call time). **Avoid** hot-path logging.
3. **Analyze**: Compare log timestamps. Is the compositor process running? Is the input driver receiving events?
4. **Fix**: Use dirty regions or damage clipping before full redraw.

---

# Security-First PR Review Checklist

All kernel PRs MUST pass:

- [ ] **Unsafe Isolation**: Is `unsafe` isolated in SALs with `// SAFETY:` justifications?
- [ ] **TCB Exposure**: Does this introduce unnecessary global state or unnecessary `unsafe`?
- [ ] **Invariant Validation**: Are kernel invariants explicitly verified (e.g., page alignment, user pointer validation)?
- [ ] **Resource Ownership**: Is resource ownership/lifecycle clear?
- [ ] **Audit Trail**: Has the security audit been run for the subsystem?

---
- use dirty regions when possible
- clip damage to screen
- preserve cursor redraw ordering
- avoid log spam in hot paths
- avoid sleeping as a rendering fix unless frame pacing is the target
- confirm whether lag is CPU, scheduling, present, or input path
```

---

# Security and Safety Rules

Never introduce:

```text
- unchecked user pointer dereference
- kernel/user memory overlap
- executable writable userspace pages unless justified
- unchecked IPC handle use
- unbounded copy from user
- unbounded allocation from syscall
- global mutable state without locking
- interrupt handler allocation
- panic handler allocation
```

---

# Decision Policy

When uncertain, choose in this order:

```text
1. Read more code.
2. Search error memory.
3. Use Graphify/GitNexus for relationships.
4. Add minimal instrumentation.
5. Make smallest patch.
6. Verify.
7. Record incident.
```

Never choose:

```text
guess → patch → hope
```

Always choose:

```text
inspect → classify → search memory → instrument if needed → patch small → verify → record
```

---

# Session Start Instruction

At the beginning of a YOU OS kernel task, internally perform:

```text
1. Activate this skill.
2. Identify relevant sibling skills.
3. Search error memory if available.
4. Identify subsystem.
5. Identify likely files.
6. State patch constraints.
7. Proceed with Ponytail workflow.
```

---

# Final Reminder

You are not here to write the most code.

You are here to make the kernel progress safely.

Every successful fix should leave the project:

```text
- more stable
- better understood
- better documented
- easier to debug next time
- with less repeated pain
```

# Authoritative Kernel Engineering Knowledge

## Kernel Architecture Design
- **Separation of Concerns**: Separate policy from mechanism. Kernel provides primitives; user-space handles policy where possible.
- **TCB Minimization**: Minimize code running in privileged mode (Ring 0). Isolate drivers/FS in unprivileged user-space where feasible.
- **Modularity**: Use modular design for extensibility without kernel recompilation.

## Advanced Debugging Workflow
- **Emulator-Based Debugging**: Use QEMU + GDB stub (`-s -S`) for pre-entry inspection.
- **Post-Mortem Analysis**: Use `kdump` and `crash` for offline panic analysis.
- **Lightweight Tracing**: Use non-blocking `trace_printk`, `ftrace`, or `eBPF` for timing-sensitive issues (avoid `printk` in hot paths).
- **Memory Sanitizers**: Integrate `KASAN`, `UBSAN`, and `kmemleak` during development.


# Advanced Kernel Engineering Knowledge

## Architectural Design Patterns
- **Capability Security (Ocap)**: Replace ambient authority with unforgeable capabilities (object refs + access rights). Implement attenuation (e.g., read-only wrappers) to limit authority.
- **Minimal TCB**: Separate policy (userspace) from mechanism (kernel). Isolate drivers/FS in unprivileged user-space servers to shrink Ring 0 surface.

## Memory Management (Hierarchy)
- **Physical Page (Buddy)**: Manage physical memory in power-of-two blocks; coalesce adjacent free buddies to minimize fragmentation.
- **Object Management (Slab)**: Pre-allocate equal-sized slabs for kernel objects (e.g., `task_struct`); use per-CPU freelists for lockless, cache-friendly allocation.

## Scheduling (CFS)
- **Fairness**: Task weight = priority. `vruntime` = (actual runtime / task weight).
- **Structure**: Red-Black Tree keyed by `vruntime` (pick smallest).

## Filesystem (VFS)
- **Journaling**: Log intent to journal *before* updating filesystem metadata to ensure crash consistency.

## IPC Optimization
- **Zero-Copy**: Map physical memory regions into both kernel and user address spaces.
- **Lock-Free**: Use atomic SPSC ring buffers with cache-line-aligned head/tail pointers (prevent false sharing).
- **Mirroring**: Map physical buffers twice in virtual space for contiguous access without explicit wrap-around.

## eBPF VM Verification
- **Verifier**: Performs static analysis (CFG validation) and abstract interpretation (safety simulation) *before* JIT compilation to guarantee runtime safety.

## Rust Kernel Safety Invariants
- **Unsafe Isolation**: Contain `unsafe` within small, SALs (Safe Abstraction Layers).
- **Safety Proofs**: Every `unsafe` block MUST have a `// SAFETY:` comment documenting *why* preconditions hold.
- **RAII**: Use `Drop` for automatic resource cleanup.
- **Explicit States**: Use `enum` state machines to prevent invalid kernel state transitions.


# Advanced Synchronization & Interrupt Patterns

## Synchronization Primitives
- **RCU (Read-Copy-Update)**: Ideal for read-heavy structures. Readers access data lock-free; writers create new versions and defer reclamation until a grace period passes.
- **Seqlocks**: Optimized for read-heavy/rare-write scenarios. Readers check a sequence counter (retry on change); writers use spinlocks.
- **Lock-Free Structs**: Use atomic primitives (`core::sync::atomic`) and cache-line alignment to prevent false sharing and contention.

## Interrupt Handling (Deferred Work)
- **Top-Half (ISR)**: Fast, non-blocking. Acknowledge hardware; schedule bottom-half.
- **Bottom-Half (Deferred)**:
  - **SoftIRQs**: High-performance, interrupt context. Used for network/block I/O.
  - **Workqueues**: Process context (CAN sleep). Most flexible for I/O and mutexes.
  - **Threaded IRQs**: Triggers a kernel-managed thread. Preferred for manageable latency.

## Rust Kernel-Space Optimizations
- **Mutex/SpinLock Integration**: Use kernel-compatible locks that integrate with `lockdep` to detect deadlocks.
- **Interrupt-Aware Locks**: For state shared with ISRs, use IRQ-disabling locks (e.g., `spin_lock_irqsave` equivalents) to prevent re-entrancy deadlocks.
- **Memory Efficiency**: Use `Pin` for fixed-address hardware objects and `Arc` (atomically reference-counted) sparingly to manage shared lifetimes.

---

# Advanced VM & Syscall ABI Design

## Virtual Memory Optimization
- **TLB Management**: The critical performance bottleneck. 
  - **Huge Pages**: Map significantly more memory per TLB entry. Prefer static, pre-allocated Huge Pages over THP (Transparent Huge Pages) in performance-critical systems to avoid latency spikes from synchronous compaction.
- **TLB Shootdowns**: Minimize IPI usage by batching invalidation flushes across cores to reduce cross-core latency.

## Syscall ABI Design Principles
- **Minimize Transitions**: Map kernel data into user-space via `vDSO` (e.g., for `gettimeofday`) to avoid mode switches for non-blocking operations.
- **Batching**: Use batch-processing interfaces (e.g., `io_uring`-style) to submit multiple requests per system call, amortizing switch overhead.
- **Security by Design**: 
  - **Least Privilege**: Syscalls must only interact with required resources.
  - **Choke Point**: Keep interface simple and implementation straightforward to minimize attack surface and simplify auditing.

---
---
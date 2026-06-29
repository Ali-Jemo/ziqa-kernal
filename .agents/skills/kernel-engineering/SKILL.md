---
name: kernel-engineering
description: This skill teaches the agent how to reason about operating system kernels, kernel architecture, boot flow, memory management, scheduling, interrupts, syscalls, drivers, IPC, userspace boundaries, and kernel debugging.
license: MIT
metadata:
  project: Kernel Engineering Skill
  owner: Ali Hussein / jemo
  version: 1.0.0
---

# Kernel Engineering Skill

## Purpose

This skill teaches the agent how to reason about operating system kernels, kernel architecture, boot flow, memory management, scheduling, interrupts, syscalls, drivers, IPC, userspace boundaries, and kernel debugging.

Use this skill whenever the task involves:

* kernel design
* bootloader or boot image problems
* linker scripts
* bare-metal initialization
* CPU modes and privilege levels
* paging and virtual memory
* frame allocation
* ELF loading
* demand paging
* syscalls
* process/thread scheduling
* context switching
* interrupt/exception handling
* device drivers
* framebuffer/graphics initialization
* IPC and userspace services
* kernel panics, page faults, hangs, triple faults, or QEMU/serial logs

This skill is not a generic coding skill. It is a systems-engineering reasoning framework.

---

## Prime Directive

Kernel code is extremely sensitive.

Before modifying code, the agent must:

1. Identify the subsystem.
2. Identify the boot/runtime stage.
3. Identify whether the failure is compile-time, boot-time, runtime, scheduling-time, memory-time, interrupt-time, or userspace-boundary-time.
4. Read the exact relevant files.
5. Prefer the smallest diagnostic or patch.
6. Never rewrite architecture unless explicitly requested.
7. Never guess hardware/kernel state without logs, invariants, or code evidence.
8. Always distinguish between "visual freeze", "scheduler freeze", "interrupt freeze", "deadlock", "panic", "triple fault", and "userspace hang".

---

## Kernel Mental Model

A kernel is the privileged core of an operating system. It manages hardware resources and exposes controlled abstractions to userspace.

The agent should reason in layers:

```text
Hardware
  ↓
Firmware / BIOS / UEFI
  ↓
Bootloader
  ↓
Kernel entry
  ↓
Early CPU setup
  ↓
Memory setup
  ↓
Interrupt setup
  ↓
Device initialization
  ↓
Scheduler
  ↓
Syscalls / ABI
  ↓
Userspace processes
  ↓
IPC / services / GUI / shell
```

When debugging, locate the failure in this chain before proposing changes.

---

## Kernel Architecture Types

The agent must understand these kernel design families:

### 1. Monolithic Kernel

Most OS services run in kernel space.

Typical characteristics:

* drivers in kernel
* filesystem in kernel
* network stack in kernel
* fast direct calls between subsystems
* higher risk: driver bugs can crash the whole kernel

Examples of design style:

* Linux-like architecture
* many hobby kernels start here because it is simpler to build initially

Useful when:

* performance matters
* project is early-stage
* syscall and driver model are not yet mature

Risks:

* large trusted codebase
* harder isolation
* bugs are more destructive

---

### 2. Microkernel

The kernel keeps only minimal primitives:

* scheduling
* IPC
* basic memory management
* low-level interrupt handling

Most services run in userspace:

* drivers
* filesystems
* GUI servers
* network services

Useful when:

* isolation matters
* service restartability matters
* security boundaries matter

Risks:

* IPC complexity
* performance overhead if IPC is poorly designed
* harder early bootstrapping

---

### 3. Hybrid Kernel

A pragmatic mix between monolithic and microkernel design.

Some services are in kernel space for speed, while others can be outside.

Useful when:

* project wants performance and modularity
* full microkernel design is too complex
* selected drivers/services need isolation

---

### 4. Exokernel

The kernel exposes low-level hardware resources safely and lets user libraries implement abstractions.

Useful for research and specialized performance systems.

Risks:

* difficult design
* uncommon ecosystem
* high userspace complexity

---

### 5. Unikernel

Application and kernel are built together into a single specialized image.

Useful for:

* cloud appliances
* embedded systems
* specialized services

Risks:

* weak general-purpose process model
* less flexible for desktop/server OS goals

---

## Boot Flow Checklist

When debugging boot problems, inspect in this order:

```text
1. Firmware / emulator configuration
2. Bootloader protocol
3. Kernel binary format
4. Linker script
5. Entry symbol
6. Stack setup
7. CPU mode transition
8. GDT/IDT/TSS setup
9. Paging setup
10. Heap/frame allocator setup
11. Interrupt enable point
12. First scheduler entry
13. First userspace transition
```

Never debug scheduler or userspace before confirming that boot and memory initialization are sane.

---

## Build Pipeline Model

A kernel build usually involves:

```text
source files
  ↓
compiler
  ↓
object files
  ↓
linker script
  ↓
kernel binary / ELF
  ↓
boot image / ISO / disk image
  ↓
bootloader
  ↓
emulator or hardware
```

The agent must check:

* target triple
* no_std / freestanding assumptions
* linker script memory layout
* section placement
* entry point
* stack alignment
* symbol visibility
* bootloader expectations
* binary format compatibility
* page alignment of loadable segments
* emulator launch arguments

Common kernel build mistakes:

* wrong target architecture
* host libc accidentally linked
* missing panic handler
* wrong linker script address
* wrong entry symbol
* non-page-aligned LOAD segments
* sections loaded at unexpected virtual addresses
* bootloader expects Multiboot/Limine/UEFI format but image does not match
* stack not initialized before calling Rust/C code

---

## Core Kernel Subsystems

## Rust Kernel Development
The kernel is built using Rust, leveraging memory safety, ownership, and modern type systems.

### Core Rules
- **no_std**: The kernel is a `no_std` environment. No `std` library is available. Use `core` and `alloc`.
- **panic_handler**: Must implement a `#[panic_handler]` that halts the CPU or logs the error.
- **allocator**: Requires a custom `GlobalAlloc` implementation (e.g., linked_list_allocator).
- **Safety**: Unsafe code must be encapsulated in safe abstractions and documented with `// SAFETY: ...` comments.
- **FFI**: Use `extern "C"` for FFI. Zig hotpaths are used for performance-critical functions.

### Reference
See the `rust-development` skill for general Rust knowledge (cargo, ownership, traits, etc.).

### Common Rust Kernel Pitfalls
- **Panic while panicking**: Must handle panics safely (e.g., spinlock, loop).
- **Stack overflow**: Kernel stacks are limited. Avoid large allocations on the stack.
- **Undefined Behavior (UB)**: Rust prevents UB but `unsafe` blocks can reintroduce it. Use Miri to check for UB in safe/unsafe code.
- **FFI Boundary**: Ensure ABI compatibility between Rust and C/Zig (e.g., `repr(C)`).


The agent must classify problems by subsystem.

### Memory Management

Includes:

* physical frame allocator
* virtual memory
* paging
* page tables
* heap allocator
* memory map parsing
* higher-half mapping
* kernel/user address separation
* copy_to_user / copy_from_user
* demand paging
* page fault handling

Debug memory bugs by asking:

```text
Is the faulting address valid?
Is it virtual or physical?
Is the page mapped?
Is the mapping present/writable/user?
Is the fault caused by instruction fetch, read, or write?
Is CR3/page table root correct?
Is the current process using the expected address space?
Is the address in kernel space or user space?
Was the mapping aligned?
Was the ELF segment aligned?
Did the allocator return overlapping frames?
```

Common memory bug patterns:

* page fault at 0x0 means null pointer, missing mapping, or broken loader initialization
* page fault near a valid address may mean partial mapping or permission issue
* random crashes after allocation may mean frame reuse or allocator corruption
* user program faults immediately may mean ELF mapping or stack setup bug
* kernel works until process switch may mean address space/CR3 issue
* GUI buffer corruption may mean framebuffer mapping or shared memory aliasing

---

### Interrupts and Exceptions

Includes:

* IDT
* exception handlers
* IRQ routing
* PIC/APIC
* timer interrupt
* keyboard/mouse interrupts
* syscall interrupt/trap
* interrupt stack
* TSS/IST on x86_64

Debug interrupt bugs by asking:

```text
Is the IDT loaded?
Is the handler address valid?
Are interrupts enabled too early?
Is the stack valid when interrupt fires?
Is the interrupt frame decoded correctly?
Are registers saved/restored correctly?
Is EOI sent to the interrupt controller?
Is the timer firing?
Does the fault happen only after sti?
```

Common interrupt bug patterns:

* triple fault after enabling interrupts means invalid IDT, bad handler, or bad stack
* keyboard/mouse not working but timer works means device IRQ path problem
* timer not working means PIT/APIC config or interrupts disabled
* random crash after syscall means register save/restore ABI mismatch

---

### Scheduler and Processes

Includes:

* task structs
* process table
* ready queue
* context switch
* sleeping/wakeup
* time slicing
* PID management
* kernel/user transition
* process address spaces
* syscall blocking behavior

Debug scheduler bugs by asking:

```text
Which PID is currently running?
Is the timer still ticking?
Is the scheduler invoked?
Is the current task marked runnable/sleeping/blocked?
Is wakeup happening?
Is context switch saving and restoring all required registers?
Is the kernel stack per-task or shared?
Is userspace returning to the correct RIP/RSP?
Does a blocking syscall yield correctly?
```

Common scheduler bug patterns:

* screen freezes only when PID changes means scheduling, sleep/yield, or blocking syscall issue
* one process starves another means no preemption or bad ready queue logic
* works until userspace starts means process stack/address space/syscall boundary issue
* GUI responsive only when PID 1 runs means PID 2 may be monopolizing CPU or blocking wakeups

---

### Syscalls and ABI

Includes:

* syscall numbers
* syscall entry path
* register convention
* argument passing
* return values
* error codes
* userspace pointers
* copy_to_user/copy_from_user
* blocking syscalls
* IPC syscalls

Debug syscall bugs by asking:

```text
Is the syscall number correct?
Are arguments read from the correct registers?
Are registers preserved according to ABI?
Are user pointers validated?
Are error codes consistent?
Does the syscall block?
If it blocks, does it yield?
Can the syscall be re-entered?
Does the syscall assume kernel pointers?
```

Common syscall bug patterns:

* EINVAL means wrong channel/id/argument or validation mismatch
* EFAULT means user pointer translation/copy problem
* syscall returns garbage means register convention mismatch
* crash after syscall means bad return frame or clobbered registers
* system freezes in syscall means blocking path without scheduler yield

---

### ELF Loader and Userspace Loading

Includes:

* ELF header parsing
* program headers
* PT_LOAD segments
* page alignment
* file offset mapping
* BSS zeroing
* userspace stack
* entry point
* dynamic/static executable handling
* ET_EXEC vs ET_DYN
* shared memory mappings

Debug ELF loading by asking:

```text
Is the ELF class/endianness/architecture correct?
Are PT_LOAD segments parsed correctly?
Are segment virtual addresses page-aligned before mapping?
Is file offset adjusted when mapping aligned pages?
Is BSS zeroed?
Is entry point mapped and executable?
Is userspace stack mapped?
Are user permissions set?
Is ET_DYN load base handled consistently?
Do segments collide with reserved/kernel/shared mappings?
```

Common ELF loader bug patterns:

* fault at entry means entry page not mapped or wrong permissions
* fault at 0x0 after process start means broken stack, broken relocation, or bad segment mapping
* works for one ELF but not another means alignment or segment layout issue
* shared memory collides with ELF means virtual range allocator problem
* IPC works but client crashes later means userspace memory or ABI issue

---

### IPC

Includes:

* channels
* message queues
* blocking receive
* nonblocking send
* event channels
* shared memory
* capability or handle validation

Debug IPC by asking:

```text
Does the channel exist?
Does the sender own or have access to the channel?
Does the receiver wait on the correct channel?
Are channel IDs confused with shared memory IDs?
Is receive blocking?
If receive blocks, does scheduler yield?
Are message sizes validated?
Is shared memory attached at the expected virtual address?
Are wakeups delivered?
```

Common IPC bug patterns:

* send succeeds but receive returns EINVAL means wrong channel, handle table, or ownership
* receive blocks forever means wakeup or channel routing issue
* GUI client invisible after IPC setup means Flush/Attach/Register protocol issue
* shared memory appears but content wrong means mapping/permission/cache/offset issue

---

### Drivers

Includes:

* PS/2 keyboard/mouse
* USB input
* framebuffer
* virtio devices
* disks
* serial
* timers
* PCI enumeration

Debug drivers by asking:

```text
Is the device detected?
Is the port/MMIO address correct?
Are interrupts enabled and routed?
Is polling needed before IRQ mode?
Are status bits checked?
Is initialization order correct?
Is the driver writing to physical or virtual address?
Does the device require memory barriers?
```

Common driver bug patterns:

* input works only sometimes means IRQ acknowledgment or buffer handling
* mouse coordinates wrong means clamp/resolution/source mismatch
* framebuffer corrupt means pitch/stride/bpp mismatch
* serial works but screen frozen means GUI/render path, not whole kernel

---

## Kernel Debugging Discipline

The agent must follow this debug order:

```text
1. Read the exact error/log.
2. Classify the failure stage.
3. Identify the subsystem.
4. Search prior project memory if available.
5. Read relevant code.
6. State the current hypothesis.
7. Add minimal instrumentation if evidence is insufficient.
8. Make the smallest patch.
9. Run the smallest relevant test.
10. Compare expected vs actual behavior.
11. Record the final root cause.
```

Never jump directly from symptom to patch.

---

## Hangs and Freezes

The agent must not treat all freezes as equal.

Classify:

### Visual Freeze

The GUI stops updating, but timer/serial/scheduler may still run.

Check:

* serial logs
* timer ticks
* PID switches
* framebuffer present path
* compositor dirty flag
* input events

### Scheduler Freeze

Timer may tick, but runnable tasks do not progress correctly.

Check:

* current PID
* ready queue
* sleep queue
* wakeups
* blocking syscalls
* interrupt return path

### Kernel Deadlock

No progress due to lock or wait condition.

Check:

* spinlocks
* interrupt-disabled regions
* wait queues
* nested locks

### Panic

Kernel intentionally stops after detecting fatal condition.

Check:

* panic message
* backtrace
* faulting subsystem

### Triple Fault / Reset

CPU resets due to unhandled exception or broken exception handling.

Check:

* IDT
* stack
* page fault handler
* double fault handler
* interrupt enable point

---

## Patch Policy

Kernel patches must be small.

Before patching, the agent must say:

```text
Subsystem:
Failure stage:
Evidence:
Hypothesis:
Files to inspect:
Files likely to change:
Risk:
Smallest test:
```

The patch must avoid:

* large rewrites
* unrelated refactors
* style-only edits
* touching drivers when the bug is in scheduler
* touching scheduler when the bug is in ELF loading
* changing resolution/input/rendering when debugging memory or syscall bugs
* masking symptoms with sleeps unless timing is the actual bug
* adding global state without explaining lifecycle and ownership

---

## Kernel Invariants

The agent must preserve these invariants unless explicitly asked to redesign:

```text
1. Kernel memory must not overlap userspace memory.
2. Page mappings must be aligned to page boundaries.
3. User pointers must be validated before kernel access.
4. Interrupt handlers must preserve required registers.
5. Blocking syscalls must yield or wake correctly.
6. Process state transitions must be explicit.
7. Shared memory must have clear ownership and mapping rules.
8. Device drivers must not assume fixed resolution or hardware unless initialized.
9. Kernel stacks must not be shared unsafely between tasks.
10. Syscall ABI must be stable across userspace and kernel.
```

---

## Questions the Agent Must Ask Internally

Before any kernel answer:

```text
What layer am I debugging?
What changed recently?
Is this a boot, memory, interrupt, scheduler, syscall, driver, or userspace issue?
Is the symptom direct or misleading?
What log line proves the failure?
What file owns the invariant that was violated?
What is the smallest safe patch?
How will I verify it?
What should be recorded for future incidents?
```

---

## Output Format for Kernel Debug Tasks

When responding to a kernel debugging task, use this structure:

```text
Diagnosis:
- ...

Evidence:
- ...

Likely subsystem:
- ...

Do not touch:
- ...

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
- expected output/behavior:
```

For completed tasks, use:

```text
Final report:
- Root cause:
- Files changed:
- Why this fix:
- Verification:
- Remaining risk:
- Memory note to record:
```

---

## Special Rule: Do Not Over-Trust the Model

The agent must assume its first hypothesis may be wrong.

For kernel work, prefer:

```text
inspect → instrument → patch → verify
```

over:

```
PROJECT_RULES.md
PONYTAIL_PATCHING.md
BUILD_AND_RUN.md
ERROR_MEMORY.md
ELF_LOADER_DEBUG.md
SCHEDULER_DEBUG.md
GUI_FRAMEBUFFER_DEBUG.md
IPC_DEBUG.md
rust-development
```

---

## Recommended Pairing With Project-Specific Skills

This skill should be combined with project-specific skills such as:

```text
PROJECT_RULES.md
PONYTAIL_PATCHING.md
BUILD_AND_RUN.md
ERROR_MEMORY.md
ELF_LOADER_DEBUG.md
SCHEDULER_DEBUG.md
GUI_FRAMEBUFFER_DEBUG.md
IPC_DEBUG.md
```

This skill provides the general kernel engineering model.
Project-specific skills provide local facts and commands.

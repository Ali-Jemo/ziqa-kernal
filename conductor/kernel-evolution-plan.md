# Axiq-IQ Kernel Evolution Plan: Towards a Full Working Kernel

This plan outlines the strategic integration of Redox OS architectural strengths into the Axiq-IQ (ZiqaKernel) codebase. The goal is to move beyond a research sandbox and build a scalable, secure, and production-ready microkernel.

## Objective
Combine Axiq-IQ's high-performance innovations (**io_uring**, **eBPF**, **ABI Plugins**) with Redox's architectural modularity (**Unified Schemes**, **Userspace Drivers**, **Scalable Contexts**).

---

## Phase 1: Infrastructure Scalability & Safety
**Goal:** Remove artificial limits and harden the user-kernel boundary.

### Changes
1.  **Dynamic Scheduler:**
    - Replace `[Option<Arc<Mutex<Process>>>; MAX_TASKS]` in `GlobalProcessTable` with a `BTreeMap<Pid, Arc<Mutex<Process>>>`.
    - Update `Scheduler::spawn` to handle dynamic allocation.
2.  **Dynamic Capabilities:**
    - Replace the fixed array in `CapabilitySpace` with a `Vec<CapabilityToken>` or `BTreeMap`.
    - Remove `MAX_CAPS_PER_PROCESS` limit.
3.  **UserCopy Audit:**
    - Ensure all syscall handlers in `src/abi/syscall.rs` use `UserSliceRo`/`UserSliceWo` for pointer dereferencing.

### Verification
- Boot the system and spawn > 64 processes (e.g., via a stress-test shell script).
- Verify capability revocation still works with the new dynamic structure.

---

## Phase 2: Unified Resource Management (The Scheme Expansion)
**Goal:** Implement the "Everything is a Scheme" philosophy for core kernel services.

### Changes
1.  **`time:` Scheme:**
    - Create `src/scheme/time.rs`.
    - Move timer management from `src/timer.rs` into this scheme.
    - Allows userspace to `read()` the current time or `sleep()` by blocking on a read.
2.  **`event:` Scheme (The Async Hub):**
    - Create `src/scheme/event.rs`.
    - Implement a registration system where processes can submit FDs to be monitored.
    - Integrate this with `io_uring` logic to provide a unified async interface.
3.  **`proc:` Scheme (Introspection):**
    - Create `src/scheme/proc.rs`.
    - Expose process list, memory usage, and CPU stats as virtual files (e.g., `proc:list`).

### Verification
- Update the shell to use `proc:list` instead of internal kernel calls to list processes.
- Test `sleep` functionality via the `time:` scheme.

---

## Phase 3: Microkernel Purity (Userspace Drivers)
**Goal:** Shift hardware management from the kernel to isolated userspace processes.

### Changes
1.  **`irq:` Scheme:**
    - Create `src/scheme/irq.rs`.
    - Allow a process with the `DeviceIo` capability to "open" an IRQ number.
    - When a hardware interrupt occurs, the kernel wakes the process waiting on that IRQ file.
2.  **Driver Migration:**
    - Move the PS/2 Keyboard driver logic from `src/drivers/` to a userspace "service" process.
    - Use the `irq:` scheme to receive key events.

### Verification
- Verify that keyboard input still works when the driver is running as a Ring 3 process.
- Monitor kernel size (it should decrease as drivers move out).

---

## Phase 4: Architecture Hardening
**Goal:** Ensure the kernel is robust enough for complex graphical applications and debugging.

### Changes
1.  **FPU/SIMD Context Switching:**
    - Update `src/arch/x86_64/switch.rs` and the `CpuState` struct.
    - Implement `fxsave`/`fxrstor` (or `xsave`/`xrstor`) during task switches.
    - This is critical for any app using floating-point math (like `doom.rs` or browser engines).
2.  **Ptrace / Debugging Interface:**
    - Implement a mechanism (via `proc:` or a new scheme) to allow one process to inspect/modify another's memory and registers.
    - Essential for bringing `gdb` or native debuggers to Axiq-IQ.

### Verification
- Run multiple instances of `doom.rs` simultaneously and check for coordinate/rendering corruption (which happens if FPU state is shared).
- Verify a simple "peek" tool can read memory from another running process.

---

## Migration & Rollback
- Each phase will be implemented in a separate feature branch.
- Reverting to the "Fixed Limit" model is possible by checking out the previous commit, though dynamic allocation is the intended permanent path.

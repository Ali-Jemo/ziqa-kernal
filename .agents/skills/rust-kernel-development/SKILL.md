---
name: rust-kernel-development
description: Use for Rust no_std kernel code: unsafe invariants, alloc/core, panic handlers, custom allocators, atomics, inline asm, MMIO, port IO, bootimage, target triples, and kernel-safe Rust patterns. Merges general rust-development discipline into bare-metal kernel constraints.
license: MIT
metadata:
  project: YOU OS / ZiqaKernel
  language: Rust
  environment: no_std kernel
---

# Rust Kernel Development

## Purpose

Apply Rust discipline inside a bare-metal `no_std` kernel. This is not normal userspace Rust.

Use together with `rust-development` for ownership, lifetimes, traits, cargo, clippy, formatting, and idiomatic Rust. This skill overrides userspace assumptions with kernel constraints.

## no_std Rules

- No `std`.
- Prefer `core`.
- Use `alloc` only after the kernel allocator is initialized.
- Never allocate in early boot, panic, interrupt handlers, or memory-manager internals unless explicitly proven safe.
- No userspace blocking abstractions.
- No file, network, thread, mutex, time, or OS APIs unless implemented by the kernel itself.

## Unsafe Rust Rules

Unsafe is allowed only where the hardware boundary requires it:

- MMIO and port I/O
- raw pointers
- page table manipulation
- user memory copy
- interrupt frames
- context switching
- inline assembly
- FFI to Zig/C
- framebuffer writes

Every unsafe block must be local, minimal, and preceded by:

```rust
// SAFETY: <exact invariant that makes this sound>
unsafe {
    ...
}
```

Before adding unsafe code, answer:

```text
Why is unsafe needed?
What invariant makes it safe?
Who owns this memory/register/device?
What alignment is required?
What lifetime is assumed?
Can interrupt/preemption happen here?
Can this pointer be userspace-controlled?
Can this fault?
```

## Kernel Ownership Hazards

Rust borrow checking does not prove kernel logic safe. Be strict with:

- `static mut`
- global state
- spinlocks
- nested locks
- interrupt-disabled sections
- references to MMIO
- references across context switches
- per-process state accessed globally
- locks held during syscall or scheduler paths

Rules:

- Do not hold locks across blocking operations.
- Do not call scheduler while holding locks unless the design explicitly permits it.
- Do not store references into objects whose lifetime crosses process/address-space switches unless pinned by kernel ownership.

## Concurrency and Atomics

Kernel concurrency can come from timer IRQs, device IRQs, syscalls, task switches, and future SMP.

Before mutating shared state:

```text
Can an interrupt mutate this?
Can another CPU/task mutate this?
Can syscall re-enter this path?
Is a spinlock enough?
Should interrupts be disabled?
What memory ordering is required?
Could this deadlock?
```

Default to boring synchronization already used in the project. Do not introduce lock-free code unless necessary and verified.

## Panic Handler Rules

Check:

- `#[panic_handler]` exists.
- It avoids allocation.
- It avoids locks that may be held.
- It logs enough serial information.
- It halts safely.

## Cargo / Build Rules

Check:

- target triple matches kernel target.
- crate is `#![no_std]` / `#![no_main]` where required.
- linker script places sections at expected addresses.
- bootimage features match task (`skip-self-tests`, `diagnostic-demo-client`).
- `rust-lld`/bootimage expectations are satisfied.

## Verification

Smallest Rust checks first:

```bash
cargo check
cargo check --features "skip-self-tests diagnostic-demo-client"
cargo build
cargo bootimage --release --features "skip-self-tests diagnostic-demo-client"
```

Do not claim runtime correctness from `cargo check` alone.

---
name: you-os-driver-debug
description: Use when debugging ZiqaKernel hardware drivers: PCI, VirtIO, ATA, NVMe, AHCI, UART, PS/2, framebuffer, xHCI, timers, IRQ routing, MMIO, DMA, memory barriers, and device initialization order.
license: MIT
metadata:
  project: YOU OS / ZiqaKernel
  subsystem: drivers
---

# YOU OS Driver Debug

## When to Use

- device not detected
- MMIO/port IO wrong
- IRQ not firing
- device works only once
- DMA buffer corruption
- framebuffer/virtio device mismatch
- disk/network device timeout
- serial works but screen frozen

## Checks

1. PCI/device detected?
2. BAR/MMIO/port address correct?
3. MMIO mapped uncached or with correct attributes?
4. IRQ routed and unmasked?
5. EOI/ack path correct?
6. Device init order follows spec?
7. DMA buffers physically contiguous/aligned as required?
8. Memory barriers needed before/after device writes?
9. Driver uses physical vs virtual address correctly?
10. Polling required before IRQ mode?

## Unsafe / Rust Rules

- MMIO register wrappers must use volatile reads/writes.
- `repr(C)`/packed structs must match hardware layout.
- Do not take unaligned references to packed fields.
- Document unsafe invariants for raw pointer register access.

## Do Not Touch

- scheduler, ELF loader, GUI compositor unless driver evidence points there.

## Minimal Plan

1. Log detection + BAR/IRQ.
2. Validate mapping and register access.
3. Check ack/EOI.
4. Patch only the first broken driver boundary.

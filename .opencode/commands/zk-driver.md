---
description: YOU OS hardware driver, PCI, VirtIO, MMIO, IRQ, and device init workflow
---

Use skill `you-os-kernel-accelerator` and `you-os-driver-debug`.

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

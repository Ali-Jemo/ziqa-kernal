# Driver Engineering Reference

## Key Concepts
- **MMIO/Port IO** - Device communication
- **IRQ Routing** - Interrupt controller management
- **DMA (Direct Memory Access)** - Device-to-memory data transfer
- **Memory Barriers** - Ensure ordering of device access
- **Device Init** - Order matters, polling vs IRQ

## Driver Debug Flow
1. **Detection** - Is device on PCI/ACPI bus?
2. **Resource Allocation** - Are BARs valid and mapped?
3. **Interrupt Routing** - Is IRQ unmasked and routed to CPU?
4. **Initialization** - Reset device, set up queues/buffers, enable.
5. **Operation** - Polling vs Interrupt-driven path.
6. **Error handling** - Device timeouts, buffer overflows.

## Checklist
- [ ] Device detected in bus enumeration?
- [ ] MMIO/Port address correct?
- [ ] MMIO mapped uncached?
- [ ] IRQs unmasked and handled?
- [ ] EOI sent?
- [ ] Init order correct (e.g., buffer setup before device enable)?
- [ ] DMA buffer aligned correctly?
- [ ] Memory barriers used before/after device access?
- [ ] Driver uses physical vs virtual address consistently?

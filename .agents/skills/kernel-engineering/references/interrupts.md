# Interrupts and Exceptions Reference

## Key Concepts
- **IDT (Interrupt Descriptor Table)** - vector table mapping interrupts to handlers
- **PIC/APIC (Programmable Interrupt Controller / Advanced PIC)** - routes IRQs to CPU
- **TSS (Task State Segment)** - IST (Interrupt Stack Table) for kernel stacks
- **EOI (End of Interrupt)** - signaled to controller to allow further interrupts

## Interrupt Debug Flow
1. **Trigger** - Hardware signal / software trap
2. **Context Save** - CPU pushes RIP, CS, RFLAGS, RSP, SS
3. **Handler** - Code in IDT executed
4. **EOI** - Controller notified
5. **Context Restore** - IRET

## Checklist
- [ ] IDT properly loaded via LIDT
- [ ] Interrupt stack valid (IST set up in TSS)
- [ ] Handlers defined for all required vectors (GPF, PF, etc.)
- [ ] PIC/APIC configured and IRQs unmasked
- [ ] Handler preserves all registers (clobbered by Rust/C call convention)
- [ ] EOI signaled for all hardware interrupts
- [ ] Interrupt enable point (`sti`) is *after* handlers are ready

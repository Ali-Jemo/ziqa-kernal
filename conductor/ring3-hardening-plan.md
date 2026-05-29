# Ring 3 Hardening & Security Audit Plan

## 1. Audit Memory Mappings (Immediate Priority)
The current `ELFLoader` indiscriminately applies `USER_ACCESSIBLE` to all LOAD segments. We must ensure that kernel-space memory and any sensitive device mappings remain in the kernel-only address space.

*   **Task 1**: Inspect `src/abi/linux/elf_loader.rs`.
    *   Currently: `user_accessible: true` is hardcoded for all loaded segments.
    *   Hardening: Validate that ELF segments intended for user-space do not overlap with kernel addresses and that kernel-level structures are explicitly excluded. (Status: **PASSED**)
*   **Task 2**: Audit `src/memory/heap.rs` and `src/drivers/virtio_net.rs`.
    *   Verify if `PageTableFlags::USER_ACCESSIBLE` is improperly applied to kernel heap or device MMIO regions. (Status: **PASSED**)

## 2. Hardening TSS and Context Switching [COMPLETED]
User-mode processes can potentially exploit the TSS if it is not correctly managed during transition.

*   **Task 1**: Review `src/arch/x86_64/gdt.rs`. [DONE]
    *   Verified TSS configuration, IST setup for double faults, and proper `TR` register loading.
*   **Task 2**: Strengthen `jump_to_user_stub` in `src/arch/x86_64/switch.rs`. [DONE]
    *   **Hardening**: Added paranoid clearing of all general-purpose registers and XMM0-XMM15 registers before `iretq` and `sysretq`. This prevents kernel data leakage into user-space.
*   **Task 3**: Audit `crate::arch::x86_64::switch::set_kernel_stack(new_proc.kernel_stack_top)` in `src/process/scheduler.rs`. [DONE]
    *   **Hardening**: Implemented `update_trap_stacks` in `src/arch/x86_64/mod.rs` to atomically update `TSS.RSP0` and the `KERNEL_STACK` global within an interrupt-disabled block. This eliminates race conditions during context switches.

## 3. Privilege Escalation Mitigation
*   **Task 1**: Enforce `rflags` sanitization.
    *   In `jump_to_user_stub`, ensure the `rflags` pushed to the stack (for `iretq`) has the `IOPL` set to 0 and `IF` (Interrupt Flag) set according to policy (usually enabled, but need to ensure it's not set maliciously by the user).

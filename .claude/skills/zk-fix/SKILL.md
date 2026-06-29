---
name: zk-fix
description: Run the YOU OS / ZiqaKernel Ponytail kernel bugfix workflow. Use for bugs, panics, freezes, page faults, scheduler issues, ELF loading, IPC, syscalls, GUI/framebuffer, QEMU, serial logs, Rust no_std kernel problems.
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-fix

Use the `you-os-kernel-accelerator` workflow.

You are debugging YOU OS / ZiqaKernel.

Arguments from user are the current bug report, log, symptom, or task.

Mandatory workflow:

1. Classify failure stage.
2. Search error memory if available.
3. Use Graphify/GitNexus if relationships are unclear.
4. Read exact files before patching.
5. Use Ponytail minimal-change policy.
6. Verify and record incident.
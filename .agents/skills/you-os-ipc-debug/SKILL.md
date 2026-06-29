---
name: you-os-ipc-debug
description: Use when debugging ZiqaKernel IPC issues: channel validation, message queues, blocking receive, nonblocking send, event channels, shared memory attach, Flush protocol, capability/handle validation.
license: MIT
metadata:
  project: YOU OS / ZiqaKernel
  subsystem: ipc
---

# YOU OS IPC Debug Skill

## When to Use

* IPC_SEND succeeds but IPC_RECV returns EINVAL
* Wrong channel / handle confusion
* Event channel failure
* Client invisible after IPC setup
* SHM attach succeeds but data wrong
* Flush ignored / compositor not notified

## Mandatory Checks

1. **Channel exists?** Check channel table.
2. **Ownership valid?** Sender/receiver own or have access.
3. **Channel vs SHM ID confusion?** Different namespaces.
4. **Receive blocking?** If so, must yield scheduler.
5. **Wakeups delivered?** Check event channel wakeup path.
5. **Message sizes validated?**
6. **SHM mapped at expected virtual address?**
7. **Flush protocol correct?** Register -> Attach -> Flush -> Present.

## Common Patterns

* **EINVAL on receive**: Wrong channel, handle table, or ownership.
* **Receive blocks forever**: Wakeup or channel routing issue.
* **GUI client invisible**: Flush/Attach/Register protocol issue.
* **SHM content wrong**: Mapping/permission/cache/offset issue.

## Do Not Touch

* Scheduler (unless receive path proven)
* ELF loader
* GUI compositor
* Drivers

Unless evidence points to IPC.

## Minimal Plan

1. Add serial log on channel create/send/recv.
2. Log channel IDs and handle table lookups.
3. Verify SHM virtual address mapping.
4. Check Flush -> wakeup path to compositor.
5. Apply smallest fix (validation, mapping, wakeup).
6. Verify with `make run-gui` + serial.
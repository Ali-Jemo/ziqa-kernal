---
name: zk-ipc
description: YOU OS IPC, SHM, event channel, Flush, and client protocol workflow
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-ipc

Use the `you-os-kernel-accelerator` workflow.

Focus on IPC/shared memory/client protocol.

Task:
$ARGUMENTS

Check:

1. Does the channel exist?
2. Does sender/receiver own the handle?
3. Are channel IDs confused with SHM IDs?
4. Is receive blocking?
5. Does blocking receive yield?
6. Are wakeups delivered?
7. Are message sizes validated?
8. Is SHM attached at expected virtual address?
9. Is RegisterEventChannel correct?
10. Is Flush handled by compositor?

Before patching, output evidence and smallest patch.
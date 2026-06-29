---
name: zk-plan
description: YOU OS kernel safe planning workflow
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# zk-plan

Use the `you-os-kernel-accelerator` workflow.

Plan a kernel change safely.

Task:
$ARGUMENTS

Rules:

1. Identify affected subsystems.
2. Check error memory for related incidents.
3. Use Graphify/GitNexus for impact analysis.
4. List all files that will be touched.
5. State verification commands.
6. Apply Ponytail minimal-change policy.
7. No architecture rewrites unless explicitly requested.
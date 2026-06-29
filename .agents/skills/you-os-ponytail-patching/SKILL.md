---
name: you-os-ponytail-patching
description: Use for ZiqaKernel/YOU OS changes where the agent must make the smallest safe patch, avoid rewrites, inspect before editing, and verify behavior with the narrowest relevant check.
license: MIT
metadata:
  project: YOU OS / ZiqaKernel
  policy: minimal-change
---

# YOU OS Ponytail Patching

## Prime Rule

Do less. Fix the source cause with the smallest safe diff.

## Fix Ladder

1. No code change: command, config, feature flag, or explanation.
2. One-line fix.
3. Small localized patch.
4. Tiny instrumentation to get evidence.
5. Subsystem patch.
6. Architecture change only when explicitly requested.

## Before Patch

Output:

```text
Subsystem:
Failure stage:
Evidence:
Hypothesis:
Files to inspect:
Files not to touch:
Smallest patch:
Verification command:
Risk:
```

## Forbidden

- rewrites during bugfixes
- unrelated formatting
- speculative abstractions
- renames during bugfixes
- changing public ABI without evidence
- touching GUI for scheduler bugs
- touching scheduler for rendering bugs
- sleeps as fake timing fixes unless frame pacing is the target
- global state without lifecycle/ownership/locking explanation

## Verification

Run the smallest command that can disprove the fix. Build success alone does not prove QEMU/runtime behavior.

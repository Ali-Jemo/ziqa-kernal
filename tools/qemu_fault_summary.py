#!/usr/bin/env python3
"""Summarize QEMU exception logs after a GUI boot crash."""
from __future__ import annotations

import re
import sys
from pathlib import Path

_EXCEPTION_RE = re.compile(
    r"v=(?P<vector>[0-9a-f]+)\s+e=(?P<error>[0-9a-f]+).*?"
    r"cpl=(?P<cpl>\d+)\s+IP=[0-9a-f]+:(?P<ip>[0-9a-f]+).*?CR2=(?P<cr2>[0-9a-f]+)",
    re.IGNORECASE,
)
_CR3_RE = re.compile(r"\bCR3=(?P<cr3>[0-9a-f]+)\b", re.IGNORECASE)


def main() -> int:
    path = Path(sys.argv[1] if len(sys.argv) > 1 else "qemu-gui-debug.log")
    if not path.exists() or path.stat().st_size == 0:
        print(f"[qemu-debug] no QEMU debug log at {path}")
        return 0

    lines = path.read_text(errors="replace").splitlines()
    fault_indices = [i for i, line in enumerate(lines) if "Triple fault" in line]
    if not fault_indices:
        print(f"[qemu-debug] no triple fault found in {path}")
        print(f"[qemu-debug] log kept at {path}")
        return 0

    triple = fault_indices[-1]
    start = max(0, triple - 55)
    window = lines[start : triple + 1]

    first_exception = None
    first_exception_index = None
    for rel, line in enumerate(window):
        if "v=0e" in line and "IP=" in line:
            match = _EXCEPTION_RE.search(line)
            if match:
                first_exception = match.groupdict()
                first_exception_index = start + rel
                break

    cr3 = None
    if first_exception_index is not None:
        for line in lines[first_exception_index : min(len(lines), first_exception_index + 22)]:
            match = _CR3_RE.search(line)
            if match:
                cr3 = match.group("cr3")
                break

    print("\n── QEMU crash summary ──")
    print(f"[qemu-debug] triple fault: {path}:{triple + 1}")
    if first_exception:
        print(
            "[qemu-debug] first page fault: "
            f"cpl={first_exception['cpl']} "
            f"rip=0x{first_exception['ip']} "
            f"cr2=0x{first_exception['cr2']} "
            f"err=0x{first_exception['error']} "
            f"cr3=0x{cr3 or 'unknown'}"
        )
        if first_exception["cpl"] == "0" and first_exception["ip"].lower() == first_exception["cr2"].lower():
            print(
                "[qemu-debug] diagnosis: kernel code page was not mapped in the active process page table."
            )
            print(
                "[qemu-debug] likely root cause: process CR3 lacks the low-half kernel mapping, "
                "so the first interrupt/page fault while running Orbital escalates to double fault."
            )
    else:
        print("[qemu-debug] first page fault was not found near the triple fault")

    print("[qemu-debug] last exception lines:")
    for line in window:
        if (
            "check_exception" in line
            or "Triple fault" in line
            or "v=0e" in line
            or "v=08" in line
            or "CR2=" in line
            or "CR3=" in line
        ):
            print(f"[qemu-debug] {line}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

# Repository Cleanup Audit Report

**Date:** 2026-06-12

## KEEP
- `src/**` source tree
- `userspace/**` source tree
- config/build manifests: `Cargo.toml`, `Cargo.lock`, `Makefile`, `linker.ld`, `build.rs`, `build.zig`, `*.json`
- docs: `docs/**`, `ZIQA_KERNEL_ROADMAP.md`, `README.md`, `ARCHITECTURE.md`
- helper scripts in use: `scripts/**`, `detect_incremental.py`, `fix_syscall.py` (only if referenced by current build/tests)

## MOVE_TO_SCRAP
- tracked runtime logs/outputs: `boot_output.txt`, `check_errors*.txt`, `make_run.log`, `qemu_*.txt`, `output.txt`, `kernel.log`, `serial.log`, `local_qemu.log`, `high_half_boot2.log`, `low_half_boot.log`
- tracked binaries/artifacts: `disk.img`, `disk2.img`, `test_boot.bin`, `orbital_probe`, `orbital_probe_2`, `redox_test`, `test_poll.o`, `test_net.o`, `libblitter.a`, `libkernel_ops.a`, `orbital.ppm`
- one-off patches/backups: `patch.rs`, `patch2.rs`, `*.bak` under `src/**`
- obsolete vendored artifacts: `redox os kernal/kernel-master.zip` and nested `redox os kernal/kernel-master/**`

## PAUSE_FOR_CONFIRMATION
- `third_party/rmm/**` is referenced by `Cargo.toml`; keep until `rmm` path dependency is removed.
- `more syscall/**` / `redox_syscall` path dependency references require confirmation.

## Next Actions
1. Expand `.gitignore` for generated artifacts if missing patterns remain.
2. Remove `MOVE_TO_SCRAP` items from tracking and working tree.
3. Re-run `node .gitnexus/run.cjs status` after cleanup.

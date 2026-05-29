# Busybox Port — Integration Notes
# userspace/BUSYBOX_PORTING.md

## Overview

BusyBox is the Phase 1 target for the ZiqaKernel userspace environment.
This document tracks the build system requirements for linking BusyBox against
`libposix.a`.

---

## Step 1 — Build libposix

```bash
cd userspace/libposix
make CC=x86_64-linux-musl-gcc   # or plain gcc for host testing
make install DESTDIR=../sysroot
```

This places:
- `../sysroot/usr/lib/libposix.a`
- `../sysroot/usr/include/posix.h`
- `../sysroot/usr/include/signals.h`

---

## Step 2 — Fetch BusyBox

```bash
wget https://busybox.net/downloads/busybox-1.36.1.tar.bz2
tar xf busybox-1.36.1.tar.bz2
cd busybox-1.36.1
```

---

## Step 3 — Configure for ZiqaKernel

```bash
make defconfig
```

Then open `.config` and set:

```ini
# Use musl-based cross-compiler targeting bare x86_64
CONFIG_CROSS_COMPILER_PREFIX="x86_64-linux-musl-"
CONFIG_SYSROOT="$(pwd)/../sysroot"

# Static binary — no dynamic linker on ZiqaKernel yet
CONFIG_STATIC=y

# Disable features that require kernel interfaces not yet implemented
CONFIG_TC=n
CONFIG_INETD=n
CONFIG_UDHCPD=n
CONFIG_FEATURE_EDITING_SAVE_ON_EXIT=n
CONFIG_FEATURE_SYSLOG=n
CONFIG_FEATURE_WTMP=n
CONFIG_FEATURE_UTMP=n
```

---

## Step 4 — Inject libposix

BusyBox's build system uses `EXTRA_CFLAGS` and `EXTRA_LDFLAGS`:

```bash
make \
  EXTRA_CFLAGS="-I$(pwd)/../sysroot/usr/include -I$(pwd)/../sysroot/usr/include/libposix" \
  EXTRA_LDFLAGS="-L$(pwd)/../sysroot/usr/lib -lposix" \
  -j$(nproc)
```

The `__attribute__((constructor))` on `libposix_init()` ensures the FD table
is initialised before `main()` runs — no changes needed to BusyBox source.

---

## Step 5 — Verify

```bash
file busybox           # should say: ELF 64-bit LSB executable, x86-64, statically linked
nm busybox | grep ziqa_syscall  # should appear — confirms libposix linked in
```

---

## Syscall Gap Analysis

The following ZiqaKernel capability syscalls **must be implemented** in
`src/abi/syscall.rs` before BusyBox will function correctly:

| Syscall # | Name              | Status       | Needed by             |
|-----------|-------------------|--------------|-----------------------|
| 1000      | `ZIQA_CAP_REQUEST`| ✅ Exists    | open()                |
| 1001      | `ZIQA_CAP_READ`   | ✅ Exists    | read()                |
| 1002      | `ZIQA_CAP_WRITE`  | ✅ Exists    | write()               |
| 1003      | `ZIQA_CAP_CLOSE`  | ⚠️ Needs impl | close()              |
| 1004      | `ZIQA_CAP_SEEK`   | ⚠️ Needs impl | lseek()              |
| 2000      | `ZIQA_SIG_SETACTION`| ⚠️ Needs impl | sigaction()         |
| 2001      | `ZIQA_SIG_GETMASK`| ⚠️ Needs impl | sigprocmask()        |
| 2002      | `ZIQA_SIG_SETMASK`| ⚠️ Needs impl | sigprocmask()        |
| 2003      | `ZIQA_SIG_KILL`   | ⚠️ Needs impl | kill()               |
| 2004      | `ZIQA_SIG_PAUSE`  | ⚠️ Needs impl | pause()              |

---

## Phase 2 — Bash Prerequisites

For Bash (phase 2), the following additional work is required:

1. **`fork()` / `exec()`**: Bash requires process cloning. The kernel's
   `sys_clone` (nr 56) and `sys_execve` (nr 59) must be fully wired through
   the ELF loader.

2. **Signal job control**: `SIGCHLD`, `SIGTSTP`, `SIGTTIN`, `SIGTTOU` must be
   deliverable — signals.c handles the userspace side; the kernel's
   `signal.rs` `SignalState::send()` is already implemented.

3. **Terminal (`tcsetpgrp`/`tcgetpgrp`)**: Bash uses these to manage process
   groups; requires a minimal PTY or VGA console termios layer.

4. **`waitpid()` / `wait4()`**: Kernel `sys_wait4` (nr 61) must populate
   `siginfo_t` with child exit status.

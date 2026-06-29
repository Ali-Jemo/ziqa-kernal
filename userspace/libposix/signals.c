// userspace/libposix/signals.c
// POSIX signal mapping → ZiqaKernel native IPC signal mechanism.
//
// How it works:
//
//   1. The kernel delivers signals via the SignalFrame mechanism
//      (src/process/signal.rs).  When a signal fires, the kernel pushes a
//      SignalFrame on the user stack and jumps to `_libposix_signal_trampoline`.
//
//   2. The trampoline calls `_libposix_signal_dispatch(signum)`, which looks
//      up the registered handler in `sig_table[]` and invokes it.
//
//   3. After the handler returns the trampoline issues SYS_RT_SIGRETURN (15)
//      to let the kernel restore the pre-signal CPU state.
//
// ZiqaKernel-specific syscall numbers used here:
//   ZIQA_SIG_SETACTION = 2000  — install sigaction in kernel's action table
//   ZIQA_SIG_GETMASK   = 2001  — read current signal mask
//   ZIQA_SIG_SETMASK   = 2002  — write signal mask (SIG_BLOCK/UNBLOCK/SETMASK)
//   ZIQA_SIG_KILL      = 2003  — send signal to pid
//   ZIQA_SIG_PAUSE     = 2004  — suspend until unblocked signal arrives

#include "signals.h"
#include "posix.h"      // ziqa_syscall is defined via posix.c (same TU group)
#include <stdint.h>
#include <stddef.h>
#include <errno.h>
#include <string.h>

// ─────────────────────────────────────────────────────────────────────────────
// ZiqaKernel signal syscall numbers
// (must match dispatch table in src/abi/syscall.rs)
// ─────────────────────────────────────────────────────────────────────────────
#define ZIQA_SIG_SETACTION   2000
#define ZIQA_SIG_GETMASK     2001
#define ZIQA_SIG_SETMASK     2002
#define ZIQA_SIG_KILL        2003
#define ZIQA_SIG_PAUSE       2004

// Linux rt_sigreturn syscall number (x86_64)
#define SYS_RT_SIGRETURN     15
// Linux getpid syscall (to implement raise())
#define SYS_GETPID           39
// Linux kill syscall (fallback for kill())
#define SYS_KILL             62

// ─────────────────────────────────────────────────────────────────────────────
// Raw syscall helper (duplicated here so signals.c is self-contained)
// ─────────────────────────────────────────────────────────────────────────────
static inline uint64_t _sig_syscall(uint64_t nr,
                                     uint64_t a0, uint64_t a1,
                                     uint64_t a2, uint64_t a3)
{
    uint64_t ret;
    register uint64_t r10 __asm__("r10") = a3;
    __asm__ volatile (
        "syscall"
        : "=a"(ret)
        : "a"(nr), "D"(a0), "S"(a1), "d"(a2), "r"(r10)
        : "rcx", "r11", "memory"
    );
    return ret;
}

static inline int is_err(uint64_t v) { return (int64_t)v < 0 && (int64_t)v >= -4095; }
static inline int to_errno(uint64_t v) { return (int)(-(int64_t)v); }

// ─────────────────────────────────────────────────────────────────────────────
// Per-process signal handler table
//   sig_table[signum-1] = current struct sigaction for that signal.
//   Protected by a spinlock because signal handlers can be set from multiple
//   threads (e.g. Bash's job-control thread vs. main thread).
// ─────────────────────────────────────────────────────────────────────────────
static struct sigaction sig_table[_NSIG];
static volatile uint32_t sig_lock = 0;

static inline void _sig_lock(void)   { while (__atomic_exchange_n(&sig_lock, 1, __ATOMIC_ACQUIRE) == 1) __asm__ volatile ("pause"); }
static inline void _sig_unlock(void) { __atomic_store_n(&sig_lock, 0, __ATOMIC_RELEASE); }

// ─────────────────────────────────────────────────────────────────────────────
// Kernel action encoding
//
// We pass action information to the kernel via ZIQA_SIG_SETACTION.
// Encoding (arg1 = action_kind):
//   0 → SIG_DFL
//   1 → SIG_IGN
//   2 → custom handler (arg2 = handler pointer)
// ─────────────────────────────────────────────────────────────────────────────
#define ACTION_DEFAULT  0
#define ACTION_IGNORE   1
#define ACTION_CUSTOM   2

static int push_to_kernel(int signum, const struct sigaction *act)
{
    uint64_t kind;
    uint64_t handler_ptr = 0;

    if (act->sa_handler == SIG_DFL) {
        kind = ACTION_DEFAULT;
    } else if (act->sa_handler == SIG_IGN) {
        kind = ACTION_IGNORE;
    } else {
        kind = ACTION_CUSTOM;
        // We give the kernel the address of our trampoline, not the user
        // handler directly — the kernel should jump here on signal delivery.
        extern void _libposix_signal_trampoline(void);
        handler_ptr = (uint64_t)_libposix_signal_trampoline;
    }

    uint64_t ret = _sig_syscall(ZIQA_SIG_SETACTION,
                                 (uint64_t)signum,
                                 kind,
                                 handler_ptr,
                                 (uint64_t)act->sa_mask);
    if (is_err(ret)) {
        errno = to_errno(ret);
        return -1;
    }
    return 0;
}

// ─────────────────────────────────────────────────────────────────────────────
// sigaction(2)
// ─────────────────────────────────────────────────────────────────────────────
int sigaction(int signum, const struct sigaction *act, struct sigaction *oldact)
{
    if (signum < 1 || signum > _NSIG) {
        errno = EINVAL;
        return -1;
    }
    // SIGKILL and SIGSTOP cannot be caught or ignored (POSIX).
    if (signum == SIGKILL || signum == SIGSTOP) {
        errno = EINVAL;
        return -1;
    }

    _sig_lock();
    if (oldact)
        *oldact = sig_table[signum - 1];

    if (act) {
        sig_table[signum - 1] = *act;
        _sig_unlock();
        // Inform the kernel about the new action.
        return push_to_kernel(signum, act);
    }
    _sig_unlock();
    return 0;
}

// ─────────────────────────────────────────────────────────────────────────────
// signal(2) — simplified sigaction wrapper
// ─────────────────────────────────────────────────────────────────────────────
sighandler_t signal(int signum, sighandler_t handler)
{
    struct sigaction act = {
        .sa_handler = handler,
        .sa_mask    = 0,
        .sa_flags   = SA_RESTART,
    };
    struct sigaction old;
    if (sigaction(signum, &act, &old) < 0)
        return SIG_ERR;
    return old.sa_handler;
}

// ─────────────────────────────────────────────────────────────────────────────
// kill(2)
// ─────────────────────────────────────────────────────────────────────────────
int kill(int pid, int sig)
{
    if (sig < 0 || sig > _NSIG) {
        errno = EINVAL;
        return -1;
    }

    // Try ZiqaKernel-native IPC first; fall back to Linux kill syscall.
    uint64_t ret = _sig_syscall(ZIQA_SIG_KILL,
                                 (uint64_t)pid,
                                 (uint64_t)sig,
                                 0, 0);
    if (is_err(ret)) {
        // Fallback: use Linux-compatible kill (nr=62).
        ret = _sig_syscall(SYS_KILL, (uint64_t)pid, (uint64_t)sig, 0, 0);
        if (is_err(ret)) {
            errno = to_errno(ret);
            return -1;
        }
    }
    return 0;
}

// ─────────────────────────────────────────────────────────────────────────────
// raise(2) — send signal to calling process
// ─────────────────────────────────────────────────────────────────────────────
int raise(int sig)
{
    uint64_t pid = _sig_syscall(SYS_GETPID, 0, 0, 0, 0);
    return kill((int)pid, sig);
}

// ─────────────────────────────────────────────────────────────────────────────
// sigprocmask(2)
// ─────────────────────────────────────────────────────────────────────────────
int sigprocmask(int how, const sigset_t *set, sigset_t *oldset)
{
    if (how != SIG_BLOCK && how != SIG_UNBLOCK && how != SIG_SETMASK) {
        errno = EINVAL;
        return -1;
    }

    // Fetch current mask from kernel.
    uint64_t cur = _sig_syscall(ZIQA_SIG_GETMASK, 0, 0, 0, 0);
    if (is_err(cur)) {
        errno = to_errno(cur);
        return -1;
    }

    if (oldset)
        *oldset = (sigset_t)cur;

    if (!set)
        return 0;  // query-only call

    uint32_t new_mask;
    switch (how) {
        case SIG_BLOCK:   new_mask = (uint32_t)cur |  *set; break;
        case SIG_UNBLOCK: new_mask = (uint32_t)cur & ~(*set); break;
        case SIG_SETMASK: new_mask = *set; break;
        default: new_mask = (uint32_t)cur; break;
    }

    // SIGKILL (9) and SIGSTOP (19) are always unblockable.
    new_mask &= ~((1u << (SIGKILL - 1)) | (1u << (SIGSTOP - 1)));

    uint64_t ret = _sig_syscall(ZIQA_SIG_SETMASK, (uint64_t)new_mask, 0, 0, 0);
    if (is_err(ret)) {
        errno = to_errno(ret);
        return -1;
    }
    return 0;
}

// ─────────────────────────────────────────────────────────────────────────────
// pause(2) — suspend until an unblocked signal is received
// ─────────────────────────────────────────────────────────────────────────────
int pause(void)
{
    _sig_syscall(ZIQA_SIG_PAUSE, 0, 0, 0, 0);
    // pause always returns -1 / EINTR when a signal is caught.
    errno = EINTR;
    return -1;
}

// ─────────────────────────────────────────────────────────────────────────────
// Signal dispatch — called by trampoline with the signal number.
// Looks up the user-installed handler and calls it.
// ─────────────────────────────────────────────────────────────────────────────
void _libposix_signal_dispatch(int signum)
{
    if (signum < 1 || signum > _NSIG)
        return;

    _sig_lock();
    struct sigaction act = sig_table[signum - 1];

    // SA_RESETHAND: reset to SIG_DFL before invoking handler.
    if (act.sa_flags & SA_RESETHAND) {
        sig_table[signum - 1].sa_handler = SIG_DFL;
        struct sigaction dfl = { .sa_handler = SIG_DFL };
        push_to_kernel(signum, &dfl);
    }
    _sig_unlock();

    if (act.sa_handler == SIG_IGN || act.sa_handler == SIG_DFL)
        return;  // kernel already handled default disposition

    act.sa_handler(signum);
}

// ─────────────────────────────────────────────────────────────────────────────
// Signal Trampoline (naked function — no prologue/epilogue)
//
// The kernel jumps here when delivering a signal.  The kernel has already:
//   1. Pushed a SignalFrame on the user stack.
//   2. Set RDI = signum.
//
// We call _libposix_signal_dispatch(signum), then issue rt_sigreturn so the
// kernel can restore the pre-signal CPU context.
// ─────────────────────────────────────────────────────────────────────────────
__attribute__((naked))
void _libposix_signal_trampoline(void)
{
    __asm__ volatile (
        // RDI already holds signum (set by the kernel).
        "call _libposix_signal_dispatch\n\t"
        // Restore pre-signal context via rt_sigreturn.
        "movq $15, %%rax\n\t"   // SYS_RT_SIGRETURN = 15
        "syscall\n\t"
        // Should never reach here.
        "ud2\n\t"
        ::: "memory"
    );
}

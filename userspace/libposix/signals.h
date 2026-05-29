// userspace/libposix/signals.h
// POSIX signal mapping for ZiqaKernel's native IPC signal mechanism.
#ifndef LIBPOSIX_SIGNALS_H
#define LIBPOSIX_SIGNALS_H

#include <stdint.h>
#include <stddef.h>

// Signal numbers (mirrors src/process/signal.rs)
#define SIGHUP    1
#define SIGINT    2
#define SIGQUIT   3
#define SIGILL    4
#define SIGTRAP   5
#define SIGABRT   6
#define SIGBUS    7
#define SIGFPE    8
#define SIGKILL   9
#define SIGUSR1   10
#define SIGSEGV   11
#define SIGUSR2   12
#define SIGPIPE   13
#define SIGALRM   14
#define SIGTERM   15
#define SIGSTKFLT 16
#define SIGCHLD   17
#define SIGCONT   18
#define SIGSTOP   19
#define SIGTSTP   20
#define SIGTTIN   21
#define SIGTTOU   22
#define SIGURG    23
#define SIGXCPU   24
#define SIGXFSZ   25
#define SIGVTALRM 26
#define SIGPROF   27
#define SIGWINCH  28
#define SIGIO     29
#define SIGPWR    30
#define SIGSYS    31
#define _NSIG     32

typedef void (*sighandler_t)(int);
#define SIG_DFL  ((sighandler_t)0)
#define SIG_IGN  ((sighandler_t)1)
#define SIG_ERR  ((sighandler_t)-1)

typedef uint32_t sigset_t;

static inline void sigemptyset(sigset_t *s) { *s = 0; }
static inline void sigfillset(sigset_t *s)  { *s = 0xFFFFFFFFu; }
static inline void sigaddset(sigset_t *s, int n) { if(n>=1&&n<=_NSIG) *s|=(1u<<(n-1)); }
static inline void sigdelset(sigset_t *s, int n) { if(n>=1&&n<=_NSIG) *s&=~(1u<<(n-1)); }
static inline int  sigismember(const sigset_t *s, int n) { return (n>=1&&n<=_NSIG)?((*s>>(n-1))&1):0; }

#define SA_RESTART   0x10000000
#define SA_NOCLDSTOP 0x00000001
#define SA_RESETHAND 0x80000000

struct sigaction {
    sighandler_t sa_handler;
    sigset_t     sa_mask;
    int          sa_flags;
};

#define SIG_BLOCK   0
#define SIG_UNBLOCK 1
#define SIG_SETMASK 2

int          sigaction(int signum, const struct sigaction *act, struct sigaction *oldact);
sighandler_t signal(int signum, sighandler_t handler);
int          kill(int pid, int sig);
int          sigprocmask(int how, const sigset_t *set, sigset_t *oldset);
int          pause(void);
int          raise(int sig);
void         _libposix_signal_dispatch(int signum);

#endif // LIBPOSIX_SIGNALS_H

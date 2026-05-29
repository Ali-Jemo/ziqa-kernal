// libposix.c - Userspace POSIX compatibility layer for ZiqaKernel
//
// This library intercepts standard POSIX syscalls (via musl or direct calls)
// and maps them to ZiqaKernel's native Capability-based VFS.

#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <errno.h>
#include <sys/types.h>

// Native ZiqaKernel syscall numbers
#define ZIQA_CAP_REQUEST 1000
#define ZIQA_CAP_READ    1001
#define ZIQA_CAP_WRITE   1002

// Raw syscall invocation (x86_64)
static inline uint64_t ziqa_syscall(uint64_t nr, uint64_t arg0, uint64_t arg1, uint64_t arg2, uint64_t arg3) {
    uint64_t ret;
    register uint64_t r10 __asm__("r10") = arg3;
    __asm__ volatile (
        "syscall"
        : "=a"(ret)
        : "a"(nr), "D"(arg0), "S"(arg1), "d"(arg2), "r"(r10)
        : "rcx", "r11", "memory"
    );
    return ret;
}

// ---------------------------------------------------------
// File Descriptor Management
// ---------------------------------------------------------

#define MAX_FDS 64

typedef struct {
    int active;
    uint64_t cap_id;
    size_t offset;
} PosixFd;

static PosixFd fd_table[MAX_FDS];

// Initialize default FDs (stdin, stdout, stderr)
void libposix_init() {
    // In a real implementation, capability tokens for 0, 1, 2 
    // would be inherited from the parent process.
    // For now, we just mark them active.
    fd_table[0].active = 1;
    fd_table[1].active = 1;
    fd_table[2].active = 1;
}

static int alloc_fd() {
    for (int i = 3; i < MAX_FDS; i++) {
        if (!fd_table[i].active) {
            fd_table[i].active = 1;
            fd_table[i].offset = 0;
            return i;
        }
    }
    return -1;
}

// ---------------------------------------------------------
// POSIX API Overrides
// ---------------------------------------------------------

int open(const char *pathname, int flags, ...) {
    // 1. Request Capability from Kernel
    uint64_t cap_id = ziqa_syscall(ZIQA_CAP_REQUEST, 
                                   1 /* ResourceKind::File */, 
                                   (uint64_t)pathname, 
                                   strlen(pathname), 
                                   flags);
    
    // Check for error (simplistic check for high error values)
    if (cap_id > 0xFFFFFFFFFFFFF000ULL) {
        errno = ENOENT; // Map kernel error to POSIX errno
        return -1;
    }

    // 2. Allocate Userspace FD
    int fd = alloc_fd();
    if (fd < 0) {
        errno = EMFILE;
        return -1;
    }

    // 3. Bind FD to Capability
    fd_table[fd].cap_id = cap_id;
    return fd;
}

ssize_t read(int fd, void *buf, size_t count) {
    if (fd < 0 || fd >= MAX_FDS || !fd_table[fd].active) {
        errno = EBADF;
        return -1;
    }

    uint64_t cap_id = fd_table[fd].cap_id;
    size_t offset = fd_table[fd].offset;

    // Call Native VFS Read
    uint64_t ret = ziqa_syscall(ZIQA_CAP_READ, cap_id, (uint64_t)buf, count, offset);

    if (ret > 0xFFFFFFFFFFFFF000ULL) {
        errno = EIO;
        return -1;
    }

    // Update internal offset
    fd_table[fd].offset += ret;
    return (ssize_t)ret;
}

ssize_t write(int fd, const void *buf, size_t count) {
    if (fd < 0 || fd >= MAX_FDS || !fd_table[fd].active) {
        errno = EBADF;
        return -1;
    }

    uint64_t cap_id = fd_table[fd].cap_id;
    size_t offset = fd_table[fd].offset;

    // Call Native VFS Write
    uint64_t ret = ziqa_syscall(ZIQA_CAP_WRITE, cap_id, (uint64_t)buf, count, offset);

    if (ret > 0xFFFFFFFFFFFFF000ULL) {
        errno = EIO;
        return -1;
    }

    // Update internal offset
    fd_table[fd].offset += ret;
    return (ssize_t)ret;
}

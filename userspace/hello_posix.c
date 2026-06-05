// userspace/hello_posix.c
//
// A simple test program for ZiqaKernel's POSIX compatibility layer.
// Linked against libposix.a.

#include "libposix/posix.h"

// Simple strlen replacement to avoid dependency on a real libc
size_t my_strlen(const char *s) {
    size_t len = 0;
    while (*s++) len++;
    return len;
}

void _start() {
    // 1. Initialize the POSIX layer (requests stdio capabilities)
    libposix_init();

    // 2. Test stdout write
    const char *msg = ">>> [libposix] Hello from userspace C program!\n";
    write(1, msg, my_strlen(msg));

    // 3. Test file creation and write
    const char *path = "posix_demo.txt";
    int fd = open(path, 0);
    if (fd >= 0) {
        const char *content = "Data written via libposix capability-based VFS.\n";
        write(fd, content, my_strlen(content));
        
        const char *ok_msg = ">>> [libposix] Successfully wrote to posix_demo.txt\n";
        write(1, ok_msg, my_strlen(ok_msg));
        
        close(fd);
    } else {
        const char *err_msg = ">>> [libposix] ERROR: Failed to open posix_demo.txt\n";
        write(1, err_msg, my_strlen(err_msg));
    }

    // 4. Exit (native syscall 60)
    __asm__ volatile (
        "mov $60, %%rax\n"
        "xor %%rdi, %%rdi\n"
        "syscall"
        : : : "rax", "rdi"
    );
}

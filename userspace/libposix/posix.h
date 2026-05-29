// userspace/libposix/posix.h
// Public header for libposix — ZiqaKernel POSIX compatibility layer.
#ifndef LIBPOSIX_H
#define LIBPOSIX_H

#include <stdint.h>
#include <stddef.h>
#include <sys/types.h>

void    libposix_init(void);

int     open(const char *pathname, int flags, ...);
ssize_t read(int fd, void *buf, size_t count);
ssize_t write(int fd, const void *buf, size_t count);
int     close(int fd);
int     dup(int oldfd);
int     dup2(int oldfd, int newfd);
off_t   lseek(int fd, off_t offset, int whence);

#endif // LIBPOSIX_H

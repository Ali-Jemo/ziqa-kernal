#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>

int main() {
    int fd = open("sys:context", O_RDONLY);
    if (fd < 0) {
        perror("ps: could not open sys:context");
        return 1;
    }

    char buf[4096];
    ssize_t n;
    while ((n = read(fd, buf, sizeof(buf) - 1)) > 0) {
        buf[n] = 0;
        printf("%s", buf);
    }

    if (n < 0) {
        perror("ps: read error");
        close(fd);
        return 1;
    }

    close(fd);
    return 0;
}

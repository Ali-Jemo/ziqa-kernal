#include "libposix/posix.h"

int main() {
    // Basic test: create a socket
    int fd = socket(2, 1, 0); // AF_INET=2, SOCK_STREAM=1
    if (fd < 0) {
        return 1; // Failure
    }
    close(fd);
    return 0;
}

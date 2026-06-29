#include "libposix/posix.h"
#include <stddef.h>

int main() {
    struct pollfd pfd;
    pfd.fd = 0; // stdin
    pfd.events = 1; // POLLIN
    
    // Poll with 10ms timeout
    int ret = poll(&pfd, 1, 10);
    
    // In our current stub kernel, poll always returns 0 unless stdin has data
    return (ret < 0) ? 1 : 0;
}

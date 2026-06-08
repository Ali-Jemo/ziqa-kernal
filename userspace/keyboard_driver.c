// userspace/keyboard_driver.c
// Userspace keyboard driver for ZiqaKernel.
// Reads scancodes using irq:1 and feeds decoded ASCII chars to keyboard: scheme.

#include "libposix/posix.h"

// Helper strlen
size_t strlen(const char *s) {
    size_t len = 0;
    while (*s++) len++;
    return len;
}

static size_t my_strlen(const char *s) {
    return strlen(s);
}

static int thread_errno = 0;
int *__errno_location(void) {
    return &thread_errno;
}

// Inline assembly helper to read from an I/O port using ZiqaKernel syscall ZIQA_DEV_PORT_IN (1031)
static uint64_t sys_port_in(uint16_t port, uint64_t size) {
    uint64_t res;
    __asm__ volatile (
        "mov $1031, %%rax\n"
        "movzwq %1, %%rdi\n"
        "mov %2, %%rsi\n"
        "syscall\n"
        "mov %%rax, %0\n"
        : "=r"(res)
        : "r"(port), "r"(size)
        : "rax", "rdi", "rsi", "rcx", "r11", "memory"
    );
    return res;
}

// Scancode tables (Set 1)
static const char scancode_map[128] = {
    0,   27,  '1', '2', '3', '4', '5', '6', '7', '8', /* 0..9 */
    '9', '0', '-', '=', '\b', /* Backspace */
    '\t',                     /* Tab */
    'q', 'w', 'e', 'r',       /* 16..19 */
    't', 'y', 'u', 'i', 'o', 'p', '[', ']', '\n', /* Enter */
    0,                        /* Control */
    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', ';', /* 30..39 */
    '\'', '`', 0,             /* Left Shift */
    '\\', 'z', 'x', 'c', 'v', 'b', 'n', 'm', ',', '.', /* 43..52 */
    '/', 0,                   /* Right Shift */
    '*',
    0,                        /* Alt */
    ' ',                      /* Space */
    0,                        /* Caps Lock */
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, /* F1..F10 */
    0,                        /* Num Lock */
    0,                        /* Scroll Lock */
    0,                        /* Home */
    0x80,                     /* Up Arrow */
    0,                        /* Page Up */
    '-',
    0x82,                     /* Left Arrow */
    0,
    0x83,                     /* Right Arrow */
    '+',
    0,                        /* End */
    0x81,                     /* Down Arrow */
    0,                        /* Page Down */
    0,                        /* Insert */
    0x88,                     /* Delete */
    0, 0, 0,
    0,                        /* F11 */
    0,                        /* F12 */
};

static const char scancode_map_shifted[128] = {
    0,   27,  '!', '@', '#', '$', '%', '^', '&', '*',
    '(', ')', '_', '+', '\b',
    '\t',
    'Q', 'W', 'E', 'R',
    'T', 'Y', 'U', 'I', 'O', 'P', '{', '}', '\n',
    0,
    'A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', ':',
    '"', '~', 0,
    '|', 'Z', 'X', 'C', 'V', 'B', 'N', 'M', '<', '>',
    '?', 0,
    '*',
    0,
    ' ',
    0,
};

void _start() {
    // 1. Initialize POSIX library
    libposix_init();

    const char *start_msg = ">>> [Userspace Keyboard] Driver started. Opening schemes...\n";
    write(1, start_msg, my_strlen(start_msg));

    // 2. Open schemes
    // Note: irq:1 gets translated to vector 33 in the kernel
    int irq_fd = open("irq:1", 0);
    if (irq_fd < 0) {
        const char *err_msg = ">>> [Userspace Keyboard] ERROR: Failed to open irq:1\n";
        write(1, err_msg, my_strlen(err_msg));
        goto exit;
    }

    int kb_fd = open("keyboard:", 0);
    if (kb_fd < 0) {
        const char *err_msg = ">>> [Userspace Keyboard] ERROR: Failed to open keyboard:\n";
        write(1, err_msg, my_strlen(err_msg));
        close(irq_fd);
        goto exit;
    }

    const char *ready_msg = ">>> [Userspace Keyboard] Driver running. Listening for interrupts...\n";
    write(1, ready_msg, my_strlen(ready_msg));

    // 3. Event loop
    int shift_held = 0;
    while (1) {
        uint64_t count = 0;
        // This blocks until a keyboard interrupt occurs
        ssize_t n = read(irq_fd, &count, sizeof(count));
        if (n <= 0) {
            // Error or EOF
            continue;
        }

        // Read raw scancode from I/O port 0x60
        uint8_t scancode = (uint8_t)sys_port_in(0x60, 1);

        // Track shift keys
        if (scancode == 0x2A || scancode == 0x36) {
            shift_held = 1;
        } else if (scancode == 0xAA || scancode == 0xB6) {
            shift_held = 0;
        } else if (scancode < 0x80) {
            char c = shift_held ? scancode_map_shifted[scancode] : scancode_map[scancode];
            if (c != 0) {
                // Write decoded character back to the keyboard scheme
                write(kb_fd, &c, 1);
            }
        }
    }

exit:
    // Exit syscall
    __asm__ volatile (
        "mov $60, %%rax\n"
        "xor %%rdi, %%rdi\n"
        "syscall"
        : : : "rax", "rdi"
    );
}

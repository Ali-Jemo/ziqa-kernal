#include <stdint.h>
#include <stddef.h>

#define ZIQA_CAP_REQUEST 1000
#define ZIQA_SHM_CREATE 1010
#define ZIQA_SHM_ATTACH 1011
#define ZIQA_IPC_SEND 1021

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

// OpCodes
#define OP_CONNECT 1
#define OP_CREATE_SURFACE 2
#define OP_FLUSH 3
#define OP_BUFFER_ATTACH 5
#define OP_SET_POSITION 6

// Structures matching ipc/gui.rs
typedef struct {
    uint64_t pid;
} ConnectMsg;

typedef struct {
    uint32_t width;
    uint32_t height;
} CreateSurfaceMsg;

typedef struct {
    uint32_t surface_id;
    uint32_t x;
    uint32_t y;
    uint32_t width;
    uint32_t height;
} FlushMsg;

typedef struct {
    uint32_t surface_id;
    uint32_t shm_id;
    uint32_t width;
    uint32_t height;
} BufferAttachMsg;

typedef struct {
    uint32_t surface_id;
    int32_t x;
    int32_t y;
} SetPositionMsg;

// Send helper
static void send_msg(uint32_t chan, uint8_t opcode, const void *payload, size_t payload_len) {
    uint8_t buf[256];
    buf[0] = opcode;
    for (size_t i = 0; i < payload_len && i < 254; i++) {
        buf[1 + i] = ((const uint8_t*)payload)[i];
    }
    ziqa_syscall(ZIQA_IPC_SEND, chan, (uint64_t)buf, 1 + payload_len, 0);
}

void _start() {
    uint32_t width = 320;
    uint32_t height = 240;
    uint32_t size = width * height * 4;

    // 1. Create SHM
    uint32_t shm_id = ziqa_syscall(ZIQA_SHM_CREATE, size, 0, 0, 0);

    // 2. Attach SHM
    uint64_t shm_addr = ziqa_syscall(ZIQA_SHM_ATTACH, shm_id, 0, 0, 0);
    uint32_t *shm_ptr = (uint32_t*)shm_addr;

    uint32_t comp_chan = 3;

    // 3. Connect
    ConnectMsg conn = { .pid = 0 };
    send_msg(comp_chan, OP_CONNECT, &conn, sizeof(conn));

    // 4. Create surface
    CreateSurfaceMsg surf = { .width = width, .height = height };
    send_msg(comp_chan, OP_CREATE_SURFACE, &surf, sizeof(surf));

    // 5. BufferAttach
    BufferAttachMsg attach = { .surface_id = 1, .shm_id = shm_id, .width = width, .height = height };
    send_msg(comp_chan, OP_BUFFER_ATTACH, &attach, sizeof(attach));

    // 6. SetPosition
    SetPositionMsg pos = { .surface_id = 1, .x = 352, .y = 264 };
    send_msg(comp_chan, OP_SET_POSITION, &pos, sizeof(pos));

    uint32_t tick = 0;
    while (1) {
        tick++;
        for (uint32_t y = 0; y < height; y++) {
            for (uint32_t x = 0; x < width; x++) {
                uint32_t r = (x + tick) & 0xFF;
                uint32_t g = (y + tick) & 0xFF;
                uint32_t b = (x + y + tick) & 0xFF;
                shm_ptr[y * width + x] = (0xFF << 24) | (r << 16) | (g << 8) | b;
            }
        }

        FlushMsg flush = { .surface_id = 1, .x = 0, .y = 0, .width = width, .height = height };
        send_msg(comp_chan, OP_FLUSH, &flush, sizeof(flush));

        // Yield/nanosleep via syscall 230 (nanosleep / 16ms)
        ziqa_syscall(230, 16, 0, 0, 0);
    }
}

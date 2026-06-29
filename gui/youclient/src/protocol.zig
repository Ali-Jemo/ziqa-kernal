pub const ZIQA_CAP_REQUEST: u64 = 1000;
pub const ZIQA_SHM_CREATE: u64 = 1010;
pub const ZIQA_SHM_ATTACH: u64 = 1011;
pub const ZIQA_IPC_CREATE: u64 = 1020;
pub const ZIQA_IPC_SEND: u64 = 1021;
pub const ZIQA_IPC_RECV: u64 = 1022;

pub const COMPOSITOR_CHAN: u32 = 3;

pub const OpCode = enum(u8) {
    Connect = 1,
    CreateSurface = 2,
    Flush = 3,
    Input = 4,
    BufferAttach = 5,
    SetPosition = 6,
    RegisterEventChannel = 7,
    // Window management
    Resize = 8,
    DestroySurface = 9,
    LowerSurface = 10,
    FocusNotify = 11,
};

pub const ConnectMsg = extern struct { pid: u64 };
pub const CreateSurfaceMsg = extern struct { width: u32, height: u32 };
pub const FlushMsg = extern struct { surface_id: u32, x: u32, y: u32, width: u32, height: u32 };
pub const InputMsg = extern struct { kind: u8, code: u32, x: i32, y: i32 };
pub const RegisterEventChannelMsg = extern struct { surface_id: u32, event_channel_id: u32 };
pub const BufferAttachMsg = extern struct { surface_id: u32, shm_id: u32, width: u32, height: u32 };
pub const SetPositionMsg = extern struct { surface_id: u32, x: i32, y: i32 };
pub const ResizeMsg = extern struct { surface_id: u32, width: u32, height: u32 };
pub const DestroySurfaceMsg = extern struct { surface_id: u32 };
pub const LowerSurfaceMsg = extern struct { surface_id: u32 };
pub const FocusNotifyMsg = extern struct { focused_id: u32 };


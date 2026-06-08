/// Display Server IPC Protocol
/// 
/// Messages sent between UI Clients and the Display Server.
/// All messages are packed into `crate::ipc::Message` (max 256 bytes).

#[repr(u8)]
pub enum OpCode {
    Connect = 1,
    CreateSurface = 2,
    Flush = 3,
    Input = 4,
}

#[repr(C)]
pub struct ConnectMsg {
    pub pid: u64,
}

#[repr(C)]
pub struct CreateSurfaceMsg {
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
pub struct FlushMsg {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
pub struct InputMsg {
    pub kind: u8, // 1=Key, 2=Mouse
    pub code: u32,
    pub x: i32,
    pub y: i32,
}

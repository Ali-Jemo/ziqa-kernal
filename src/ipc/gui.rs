/// Display Server IPC Protocol
/// 
/// Messages sent between UI Clients and the Display Server.
/// All messages are packed into `crate::ipc::Message` (max 256 bytes).

/// Opcodes for the display server protocol.
/// Sent as the first byte of every IPC message.
#[repr(u8)]
pub enum OpCode {
    Connect = 1,
    CreateSurface = 2,
    Flush = 3,
    Input = 4,
    BufferAttach = 5,
    SetPosition = 6,
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
/// Mark a rectangular region of a surface as dirty (needs repaint).
/// The `surface_id` identifies which surface to mark.
#[repr(C)]
pub struct FlushMsg {
    pub surface_id: u32,
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

/// Attach a SHM buffer to an existing surface.
/// Sent by the client after creating a surface and an SHM segment.
#[repr(C)]
pub struct BufferAttachMsg {
    pub surface_id: u32,
    pub shm_id: u32,
    pub width: u32,
    pub height: u32,
}

/// Set the position of a surface on screen.
#[repr(C)]
pub struct SetPositionMsg {
    pub surface_id: u32,
    pub x: i32,
    pub y: i32,
}

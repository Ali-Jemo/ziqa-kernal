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
    RegisterEventChannel = 7,
    // ── Window management ──────────────────────────────────────────────
    Resize = 8,
    DestroySurface = 9,
    LowerSurface = 10,
    FocusNotify = 11,
    SetWindowKind = 12,
    SetCursorShape = 13,
}

#[repr(C)]
pub struct ConnectMsg {
    pub pid: u64,
}

#[repr(C)]
pub struct CreateSurfaceMsg {
    pub surface_id: u32,
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

#[repr(C)]
pub struct RegisterEventChannelMsg {
    pub surface_id: u32,
    pub event_channel_id: u32,
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

/// Request a surface resize. Client must re-allocate SHM and re-attach.
#[repr(C)]
pub struct ResizeMsg {
    pub surface_id: u32,
    pub width: u32,
    pub height: u32,
}

/// Request surface destruction (client-initiated close).
#[repr(C)]
pub struct DestroySurfaceMsg {
    pub surface_id: u32,
}

/// Move a surface one step down in z-order (behind its current neighbor).
#[repr(C)]
pub struct LowerSurfaceMsg {
    pub surface_id: u32,
}

/// Sent server→client when focus changes. `focused_id` is the newly focused
/// surface (0 = no surface focused).
#[repr(C)]
pub struct FocusNotifyMsg {
    pub focused_id: u32,
}


#[repr(C)]
pub struct SetWindowKindMsg {
    pub surface_id: u32,
    pub kind: u8, // 0=Floating, 1=Tiled, 2=Dialog, 3=Popup
}

#[repr(C)]
pub struct SetCursorShapeMsg {
    pub surface_id: u32,
    pub shape: u8, // 0=Default, 1=Text, 2=Hidden
}
